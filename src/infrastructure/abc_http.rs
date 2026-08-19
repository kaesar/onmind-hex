//! XDB `/abc` client over HTTP using reqwest (feature `abc`).
//! Mirrors hex4w `AbcWebClient`/`AbcAdapter` for both the sheet read path and
//! the `exec` write path, with optional HTTP auth. The default credential is
//! `admin:admin` (hex4w's embedded XDB) unless overridden.
//!
//! Env:
//! - `HEX_XDB_AUTH_TYPE`  `basic` | `bearer` (default `basic`)
//! - `HEX_XDB_AUTH_TOKEN` bearer token, or `user:pass` for basic (default `admin:admin`)

use crate::application::ports::{AbcPort, AbcRequest};
use crate::domain::{AbcResponse, DomainError};

#[derive(Clone)]
enum Auth {
    None,
    Basic(String, String),
    Bearer(String),
}

#[derive(Clone)]
pub struct XdbHttpAdapter {
    client: reqwest::blocking::Client,
    base_url: String,
    auth: Auth,
}

fn auth_from_env() -> Auth {
    let auth_type = std::env::var("HEX_XDB_AUTH_TYPE").unwrap_or_else(|_| "basic".into());
    let token = std::env::var("HEX_XDB_AUTH_TOKEN").unwrap_or_else(|_| "admin:admin".into());
    match auth_type.as_str() {
        "bearer" => Auth::Bearer(token),
        "none" => Auth::None,
        _ => {
            let (user, pass) = match token.split_once(':') {
                Some((u, p)) => (u.to_string(), p.to_string()),
                None => ("admin".into(), token),
            };
            Auth::Basic(user, pass)
        }
    }
}

fn post(
    client: &reqwest::blocking::Client,
    url: &str,
    auth: &Auth,
    body: &serde_json::Value,
) -> Result<reqwest::blocking::Response, reqwest::Error> {
    let mut req = client.post(url).json(body);
    req = match auth {
        Auth::None => req,
        Auth::Basic(u, p) => req.basic_auth(u, Some(p)),
        Auth::Bearer(t) => req.bearer_auth(t),
    };
    req.send()
}

impl XdbHttpAdapter {
    pub fn new(base_url: impl Into<String>) -> Result<Self, reqwest::Error> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        Ok(Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            auth: auth_from_env(),
        })
    }

    fn post_json(&self, payload: &serde_json::Value) -> Result<AbcResponse, DomainError> {
        let url = format!("{}/abc", self.base_url);
        let resp = post(&self.client, &url, &self.auth, payload)
            .map_err(|e| DomainError::Internal(format!("xdb post: {e}")))?;
        resp.json::<AbcResponse>()
            .map_err(|e| DomainError::Internal(format!("xdb decode: {e}")))
    }
}

impl AbcPort for XdbHttpAdapter {
    fn sheet(&self, show: &str, from: &str, some: &str) -> Result<AbcResponse, DomainError> {
        self.post_json(&serde_json::json!({
            "what": "find",
            "from": from,
            "some": some,
            "show": show,
        }))
    }

    fn exec(&self, req: &AbcRequest) -> Result<AbcResponse, DomainError> {
        self.post_json(&serde_json::to_value(req).map_err(|e| DomainError::Internal(format!("xdb exec serialize: {e}")))?)
    }
}