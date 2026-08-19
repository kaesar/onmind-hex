//! Composition root: assemble the output-port adapters, the `services` facade,
//! the engine and the scripting use case once, then share them.

use std::path::PathBuf;
use std::sync::Arc;

use crate::application::ports::{
    AbcPort, AbcRequest, CachePort, EmailPort, EventPort, RoleRepositoryPort, StorePort,
};
use crate::application::{FacadeBuilder, ScriptCommandUseCase, ScriptWhitelist, ScriptingUseCase};
#[cfg(feature = "lambda")]
use crate::application::ports::LambdaPort;
use crate::domain::{AbcResponse, DomainError, StoreItem};
use crate::infrastructure::circuit_breaker::{CbConfig, CircuitBreaker};
#[cfg(feature = "abc")]
use crate::infrastructure::cb_decorator::CbAbcPort;
use crate::infrastructure::cb_decorator::CbCachePort;
use crate::infrastructure::engine::{AbcodeEngine, ServicesConfig};
use crate::infrastructure::source::ScriptSource;

pub struct Graph {
    pub scripting: Arc<ScriptingUseCase>,
    abc: Option<Arc<dyn AbcPort>>,
    store: Option<Arc<dyn StorePort>>,
    email: Option<Arc<dyn EmailPort>>,
    roles: Option<Arc<dyn RoleRepositoryPort>>,
    script_commands: Option<Arc<ScriptCommandUseCase>>,
}

impl Graph {
    /// Reads config from the environment and wires the app.
    ///
    /// Env:
    /// - `HEX_SCRIPTS_DIR`        (default `./scripts`)
    /// - `HEX_SCRIPTS_WHITELIST`  (comma-separated, default `hello.abc`)
    /// - `HEX_XDB_BASE_URL`       (feature `abc`, default `http://localhost:9990`)
    /// - `HEX_XDB_GRPC_ENABLED`   (features `abc`+`grpc`): use the gRPC client
    /// - `HEX_XDB_GRPC_URL`       (default `http://localhost:9991`)
    /// - `HEX_XDB_CACHE_TTL`      (feature `cache`, default 300)
    /// - `HEX_REDIS_URL`          (feature `cache`, default `redis://127.0.0.1:6379`)
    /// - `HEX_EVENTS_TYPE`        sns|sqs|eventbridge|kafka|rabbit|none
    pub fn from_env() -> Self {
        let dir = std::env::var("HEX_SCRIPTS_DIR").unwrap_or_else(|_| "./scripts".into());
        let whitelist_csv =
            std::env::var("HEX_SCRIPTS_WHITELIST").unwrap_or_else(|_| "hello.abc".into());
        let whitelist = ScriptWhitelist::from_csv(&whitelist_csv);

        let source = ScriptSource::new(PathBuf::from(dir));
        let mut builder = FacadeBuilder::new();

        #[allow(unused_mut)]
        let mut abc: Option<Arc<dyn AbcPort>> = None;
        #[allow(unused_mut)]
        let mut store: Option<Arc<dyn StorePort>> = None;
        #[allow(unused_mut, unused_assignments)]
        let mut events: Option<Arc<dyn EventPort>> = None;
        #[allow(unused_mut)]
        let mut email: Option<Arc<dyn EmailPort>> = None;
        #[allow(unused_mut)]
        let mut roles: Option<Arc<dyn RoleRepositoryPort>> = None;

        // Cache: real Redis when reachable, otherwise in-memory. Behind a CB too.
        {
            let inner: Arc<dyn CachePort> = redis_cache();
            let cb = Arc::new(CircuitBreaker::new(CbConfig::default()));
            let cache: Arc<dyn CachePort> = Arc::new(CbCachePort::new(inner, cb));
            builder = builder.with_cache(cache);
        }

        // XDB: HTTP or gRPC client, cache-first, then circuit-broken.
        #[cfg(feature = "abc")]
        {
            let raw: Option<Arc<dyn AbcPort>> = xdb_client();
            if let Some(raw) = raw {
                #[cfg(feature = "cache")]
                let raw = {
                    let ttl = env_u64("HEX_XDB_CACHE_TTL", 300);
                    let cache: Arc<dyn CachePort> = builder_cache();
                    let cached = crate::infrastructure::CachedAbcPort::new(raw, cache, ttl);
                    Arc::new(cached) as Arc<dyn AbcPort>
                };
                let breaker = Arc::new(CircuitBreaker::new(cb_config()));
                let wrapped: Arc<dyn AbcPort> =
                    Arc::new(CbAbcPort::new(raw, breaker));
                abc = Some(Arc::clone(&wrapped));
                builder = builder.with_abc(wrapped);
            } else {
                eprintln!("[hex] xdb adapter not initialized");
            }
        }

        // Cloud adapters that need a Tokio runtime.
        #[cfg(any(
            feature = "store",
            feature = "events",
            feature = "events-sqs",
            feature = "events-eventbridge",
            feature = "events-kafka",
            feature = "events-rabbit",
            feature = "lambda"
        ))]
        {
            let rt = crate::infrastructure::cloud_runtime();

            #[cfg(feature = "store")]
            if let Ok(bucket) = std::env::var("HEX_STORE_BUCKET") {
                let s: Arc<dyn StorePort> = Arc::new(
                    crate::infrastructure::S3Store::new(bucket, Arc::clone(&rt)),
                );
                store = Some(Arc::clone(&s));
                builder = builder.with_store(s);
            }

            #[cfg(any(
                feature = "events",
                feature = "events-sqs",
                feature = "events-eventbridge",
                feature = "events-kafka",
                feature = "events-rabbit"
            ))]
            {
                events = event_publisher(Arc::clone(&rt));

                if let Some(events) = &events {
                    builder = builder.with_events(Arc::clone(events));
                }
            }

            #[cfg(feature = "lambda")]
            {
                let lambda: Arc<dyn LambdaPort> = Arc::new(
                    crate::infrastructure::LambdaAdapter::new(Arc::clone(&rt)),
                );
                builder = builder.with_lambda(lambda);
            }
        }

        // Email (SMTP) is blocking (no Tokio).
        #[cfg(feature = "email")]
        if let Ok(smtp) = crate::infrastructure::SmtpEmailSender::new_from_env() {
            let e: Arc<dyn EmailPort> = Arc::new(smtp);
            email = Some(Arc::clone(&e));
            builder = builder.with_email(e);
        } else {
            eprintln!("[hex] smtp adapter not enabled (set HEX_SMTP_HOST/FROM)");
        }

        // Role persistence (SQLite).
        #[cfg(feature = "db")]
        match crate::infrastructure::SqliteRoleRepository::open() {
            Ok(repo) => {
                let r: Arc<dyn RoleRepositoryPort> = Arc::new(repo);
                roles = Some(r);
            }
            Err(e) => eprintln!("[hex] roles db init failed: {e}"),
        }

        let services = builder.build();
        let engine = AbcodeEngine::new(Arc::new(ServicesConfig::new(services)));
        let scripting = Arc::new(ScriptingUseCase::new(whitelist, source, engine));

        let script_commands = events.map(|events| {
            let results_topic = std::env::var("HEX_SCRIPT_RESULTS_TOPIC")
                .unwrap_or_else(|_| "hex4w.script.results".into());
            Arc::new(ScriptCommandUseCase::new(
                Arc::clone(&scripting),
                events,
                results_topic,
            ))
        });

        Self {
            scripting,
            abc,
            store,
            email,
            roles,
            script_commands,
        }
    }

    /// XDB sheet read (hex4w `GET /api/v1/xdb/sheet`).
    pub fn abc_sheet(
        &self,
        show: &str,
        from: &str,
        some: &str,
    ) -> Result<AbcResponse, DomainError> {
        match &self.abc {
            Some(p) => p.sheet(show, from, some),
            None => Err(DomainError::Internal(
                "xdb adapter not enabled (feature 'abc')".into(),
            )),
        }
    }

    /// XDB write/exec (hex4w `services.abcExec`).
    pub fn abc_exec(&self, req: &AbcRequest) -> Result<AbcResponse, DomainError> {
        match &self.abc {
            Some(p) => p.exec(req),
            None => Err(DomainError::Internal(
                "xdb adapter not enabled (feature 'abc')".into(),
            )),
        }
    }

    /// `services.listItems`/`GET /api/v1/store/items?bucket=` (feature `store`).
    pub fn list_items(&self, bucket: &str) -> Result<Vec<StoreItem>, DomainError> {
        match &self.store {
            Some(s) => s.list_items(bucket),
            None => Err(DomainError::Internal("store adapter not enabled (feature 'store')".into())),
        }
    }

    /// `services.sendEmail`/`POST /api/v1/notifications/email` (feature `email`).
    pub fn send_email(&self, to: &str, subject: &str, body: &str) -> Result<(), DomainError> {
        match &self.email {
            Some(e) => e.send_email(to, subject, body),
            None => Err(DomainError::Internal("email adapter not enabled (feature 'email')".into())),
        }
    }

    /// Role repository (feature `db`).
    pub fn roles(&self) -> Option<&Arc<dyn RoleRepositoryPort>> {
        self.roles.as_ref()
    }

    /// Script-command consumer use case (only when an event publisher is wired).
    pub fn script_commands(&self) -> Option<Arc<ScriptCommandUseCase>> {
        self.script_commands.clone()
    }
}

#[cfg(feature = "abc")]
fn builder_cache() -> Arc<dyn CachePort> {
    let inner = redis_cache();
    let cb = Arc::new(CircuitBreaker::new(CbConfig::default()));
    Arc::new(CbCachePort::new(inner, cb))
}

/// Redis when reachable; in-memory fallback otherwise.
fn redis_cache() -> Arc<dyn CachePort> {
    #[cfg(feature = "cache")]
    {
        let url =
            std::env::var("HEX_REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
        if let Ok(rc) = crate::infrastructure::RedisCache::connect(&url) {
            println!("- cache: redis ({url})");
            return Arc::new(rc);
        }
    }
    println!("- cache: in-memory");
    Arc::new(crate::infrastructure::InMemoryCache::default())
}

/// HTTP or gRPC XDB client, selected by `HEX_XDB_GRPC_ENABLED`.
#[cfg(feature = "abc")]
fn xdb_client() -> Option<Arc<dyn AbcPort>> {
    #[cfg(feature = "grpc")]
    {
        let enabled = std::env::var("HEX_XDB_GRPC_ENABLED")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);
        if enabled {
            let url = std::env::var("HEX_XDB_GRPC_URL")
                .unwrap_or_else(|_| "http://localhost:9991".into());
            return match crate::infrastructure::grpc_abc::GrpcAbcClient::new(url.clone()) {
                Ok(c) => {
                    println!("- xdb: gRPC client ({url})");
                    Some(Arc::new(c))
                }
                Err(e) => {
                    eprintln!("[hex] grpc xdb client init failed: {e}");
                    None
                }
            };
        }
    }
    let base = std::env::var("HEX_XDB_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:9990".into());
    match crate::infrastructure::XdbHttpAdapter::new(base.clone()) {
        Ok(a) => {
            println!("- xdb: HTTP client ({base})");
            Some(Arc::new(a))
        }
        Err(e) => {
            eprintln!("[hex] xdb adapter init failed: {e}");
            None
        }
    }
}

/// Selects the event publisher from `HEX_EVENTS_TYPE` (hex4w profile switch).
#[cfg(any(
    feature = "events",
    feature = "events-sqs",
    feature = "events-eventbridge",
    feature = "events-kafka",
    feature = "events-rabbit"
))]
fn event_publisher(rt: Arc<tokio::runtime::Runtime>) -> Option<Arc<dyn EventPort>> {
    let wanted = std::env::var("HEX_EVENTS_TYPE")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "sns".into());

    match wanted.as_str() {
        "none" => None,
        #[cfg(feature = "events")]
        "sns" => {
            let p = crate::infrastructure::SnsEventPublisher::new(rt);
            println!("- events: SNS");
            Some(Arc::new(p))
        }
        #[cfg(feature = "events-sqs")]
        "sqs" => match crate::infrastructure::SqsEventPublisher::new(rt) {
            Ok(p) => {
                println!("- events: SQS");
                Some(Arc::new(p))
            }
            Err(e) => {
                eprintln!("[hex] sqs publisher init failed: {e}");
                None
            }
        },
        #[cfg(feature = "events-eventbridge")]
        "eventbridge" => {
            let p = crate::infrastructure::EventBridgeEventPublisher::new(rt);
            println!("- events: EventBridge");
            Some(Arc::new(p))
        }
        #[cfg(feature = "events-kafka")]
        "kafka" => match crate::infrastructure::KafkaEventPublisher::new() {
            Ok(p) => {
                println!("- events: Kafka");
                Some(Arc::new(p))
            }
            Err(e) => {
                eprintln!("[hex] kafka publisher init failed: {e}");
                None
            }
        },
        #[cfg(feature = "events-rabbit")]
        "rabbit" => match crate::infrastructure::RabbitEventPublisher::new(rt) {
            Ok(p) => {
                println!("- events: RabbitMQ");
                Some(Arc::new(p))
            }
            Err(e) => {
                eprintln!("[hex] rabbit publisher init failed: {e}");
                None
            }
        },
        other => {
            eprintln!("[hex] unknown HEX_EVENTS_TYPE '{other}' (available: sns, sqs, eventbridge, kafka, rabbit, none)");
            None
        }
    }
}

/// Circuit breaker tuning from env (hex4w `CircuitBreakerProperties`).
pub fn cb_config() -> CbConfig {
    CbConfig {
        failure_threshold: env_usize("HEX_CB_FAILURE_THRESHOLD", 5),
        reset_timeout: std::time::Duration::from_millis(env_u64("HEX_CB_RESET_MS", 500)),
        half_open_max: env_usize("HEX_CB_HALF_OPEN_MAX", 1),
    }
}

fn env_usize(k: &str, default: usize) -> usize {
    std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}
fn env_u64(k: &str, default: u64) -> u64 {
    std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}
