//! XDB `/abc` client over HTTP using reqwest (feature `abc`).
//! Mirrors hex4w `AbcWebClient`/`AbcAdapter` for the sheet read path.

use crate::application::ports::AbcPort;
use crate::domain::{AbcResponse, DomainError};

#[derive(Clone)]
pub struct XdbHttpAdapter {
    client: reqwest::blocking::Client,
    base_url: String,
}

impl XdbHttpAdapter {
    pub fn new(base_url: impl Into<String>) -> Result<Self, reqwest::Error> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        Ok(Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        })
    }
}

impl AbcPort for XdbHttpAdapter {
    fn sheet(&self, show: &str, from: &str, some: &str) -> Result<AbcResponse, DomainError> {
        let url = format!("{}/abc", self.base_url);
        let payload = serde_json::json!({
            "what": "find",
            "from": from,
            "some": some,
            "show": show,
        });
        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .map_err(|e| DomainError::Internal(format!("xdb sheet: {e}")))?;
        resp.json::<AbcResponse>()
            .map_err(|e| DomainError::Internal(format!("xdb sheet decode: {e}")))
    }
}