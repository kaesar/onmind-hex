use feather::*;
use feather::jwt::{JwtManager, SimpleClaims};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::sync::Arc;

use hex::domain::domain_status;
use hex::graph::Graph;

const DEFAULT_PORT: u16 = 3001;
const VERSION: &str = "0.1.0";

#[derive(Deserialize)]
struct ExecuteRequest {
    script: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    code: String,
    message: String,
    status: u16,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
    version: &'static str,
    auth_mode: &'static str,
}

fn main() {
    let graph = Arc::new(Graph::from_env());

    let mut app = App::new();

    let auth_config = AuthConfig::from_env();
    let auth_mode = if auth_config.jwt_manager.is_some() { "JWT" } else { "NoAuth" };
    app.use_middleware(auth_middleware(auth_config));

    // POST /api/v1/script/execute      { "script": "hello.abc" } -> ScriptResult
    {
        let graph = Arc::clone(&graph);
        app.post("/api/v1/script/execute", move |req: &mut Request, res: &mut Response, _: &AppContext| -> Result<MiddlewareResult, Box<dyn Error>> {
            let body = std::str::from_utf8(&req.body).unwrap_or("");
            let request: ExecuteRequest = match serde_json::from_str(body) {
                Ok(r) => r,
                Err(e) => {
                    json_error(res, 400, "INVALID_REQUEST", &format!("{e}"));
                    return Ok(MiddlewareResult::Next);
                }
            };

            match graph.scripting.execute(&request.script) {
                Ok(result) => json_body(res, 200, &result),
                Err(e) => {
                    let status = domain_status(&e);
                    json_error(res, status, error_code(&e), &e.to_string());
                }
            }
            Ok(MiddlewareResult::Next)
        });
    }

    // GET /api/v1/xdb/sheet?show=&from=&some=
    {
        let graph = Arc::clone(&graph);
        app.get("/api/v1/xdb/sheet", move |req: &mut Request, res: &mut Response, _: &AppContext| -> Result<MiddlewareResult, Box<dyn Error>> {
            let show = req.param("show").unwrap_or("");
            let from = req.param("from").unwrap_or("xykit");
            let some = req.param("some").unwrap_or("sheet");
            match graph.abc_sheet(show, from, some) {
                Ok(r) => json_body(res, 200, &r),
                Err(e) => json_error(res, domain_status(&e), error_code(&e), &e.to_string()),
            }
            Ok(MiddlewareResult::Next)
        });
    }

    // GraphQL BFF (feature `graphql`)
    #[cfg(feature = "graphql")]
    {
        use async_graphql::Request as GqlRequest;
        let graph = Arc::clone(&graph);
        let schema = hex::graphql::build_schema();
        app.post("/graphql", move |req: &mut Request, res: &mut Response, _: &AppContext| -> Result<MiddlewareResult, Box<dyn Error>> {
            #[derive(serde::Deserialize)]
            struct GqlBody {
                #[serde(default)]
                query: String,
                #[serde(default)]
                operation_name: Option<String>,
                #[serde(default)]
                variables: Option<serde_json::Value>,
            }
            let body = std::str::from_utf8(&req.body).unwrap_or("{}");
            let parsed: GqlBody = match serde_json::from_str(body) {
                Ok(p) => p,
                Err(e) => {
                    json_error(res, 400, "INVALID_REQUEST", &format!("{e}"));
                    return Ok(MiddlewareResult::Next);
                }
            };
            let mut gql = GqlRequest::new(parsed.query);
            if let Some(op) = parsed.operation_name {
                gql = gql.operation_name(op);
            }
            if let Some(vars) = parsed.variables {
                if let Ok(map) = serde_json::from_value(vars) {
                    gql = gql.variables(map);
                }
            }
            let state = hex::graphql::AppState { graph: Arc::clone(&graph) };
            let resp = futures::executor::block_on(schema.execute(gql.data(state)));
            json_body(res, 200, &resp);
            Ok(MiddlewareResult::Next)
        });
        println!("   POST /graphql (GraphQL BFF)");
    }

    // GET /health
    app.get("/health", move |_req: &mut Request, res: &mut Response, _: &AppContext| -> Result<MiddlewareResult, Box<dyn Error>> {
        let h = HealthResponse {
            status: "healthy",
            service: "hex",
            version: VERSION,
            auth_mode,
        };
        json_body(res, 200, &h);
        Ok(MiddlewareResult::Next)
    });

    let port = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT);

    #[cfg(feature = "grpc")]
    hex::grpc::serve(&graph);

    println!("- hex - Hexagonal ABCode scripting service");
    println!("- Env: server http://localhost:{port}");
    println!("- Endpoints:");
    println!("   POST /api/v1/script/execute");
    println!("   GET  /api/v1/xdb/sheet");
    println!("   GET  /health");

    app.listen(format!("127.0.0.1:{port}"));
}

fn error_code(e: &hex::domain::DomainError) -> &'static str {
    match e {
        hex::domain::DomainError::ScriptNotAllowed(_) => "SCRIPT_NOT_ALLOWED",
        hex::domain::DomainError::ScriptNotFound(_) => "SCRIPT_NOT_FOUND",
        hex::domain::DomainError::InvalidRequest(_) => "INVALID_REQUEST",
        hex::domain::DomainError::Unavailable(_) => "SERVICE_UNAVAILABLE",
        hex::domain::DomainError::Internal(_) => "INTERNAL_ERROR",
    }
}

fn json_body(res: &mut Response, status: u16, value: &impl Serialize) {
    match serde_json::to_string(value) {
        Ok(s) => {
            res.set_status(status);
            res.body = Some(s.into_bytes().into());
            let _ = res.add_header("Content-Type", "application/json");
        }
        Err(_) => {
            res.set_status(500);
            res.body = Some(b"{\"error\":\"serialization\"}".to_vec().into());
        }
    }
}

fn json_error(res: &mut Response, status: u16, code: &str, message: &str) {
    let body = ErrorResponse {
        code: code.to_string(),
        message: message.to_string(),
        status,
    };
    json_body(res, status, &body);
}

// ---- Authentication (JWT / NoAuth), same contract as abcodefun -------------------

struct AuthConfig {
    jwt_manager: Option<JwtManager>,
}

impl AuthConfig {
    fn from_env() -> Self {
        match std::env::var("JWT_SECRET") {
            Ok(secret) => {
                println!("- JWT Authentication: ENABLED (HS256)");
                AuthConfig { jwt_manager: Some(JwtManager::new(secret)) }
            }
            Err(_) => {
                println!("- JWT Authentication: DISABLED (NoAuth mode)");
                AuthConfig { jwt_manager: None }
            }
        }
    }
}

fn auth_middleware(
    auth_config: AuthConfig,
) -> impl Fn(&mut Request, &mut Response, &AppContext) -> Result<MiddlewareResult, Box<dyn Error>> + 'static
{
    move |req, res, _ctx| -> Result<MiddlewareResult, Box<dyn Error>> {
        let Some(manager) = &auth_config.jwt_manager else {
            return Ok(MiddlewareResult::Next);
        };

        // Health is always public (matches abcodefun).
        if req.path() == "/health" {
            return Ok(MiddlewareResult::Next);
        }

        let auth_header = req
            .headers
            .get("authorization")
            .and_then(|h| h.to_str().ok());
        let token = match auth_header {
            Some(header) if header.starts_with("Bearer ") => &header[7..],
            _ => {
                json_error(res, 401, "UNAUTHORIZED", "Missing or invalid Authorization header");
                return Ok(MiddlewareResult::End);
            }
        };

        match manager.decode::<SimpleClaims>(token) {
            Ok(claims) => {
                req.extensions.insert(Arc::new(claims));
                Ok(MiddlewareResult::Next)
            }
            Err(e) => {
                json_error(res, 401, "UNAUTHORIZED", &format!("Invalid token: {e}"));
                Ok(MiddlewareResult::End)
            }
        }
    }
}