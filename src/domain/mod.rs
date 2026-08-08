//! Pure domain models (no framework / no I/O).

use serde::{Deserialize, Serialize};

/// Result of executing an ABCode script: its completion value, captured console
/// output (`stdout`) and an optional error message (`stderr`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptResult {
    /// JSON-comparable completion value of the script, when defined.
    pub value: Option<serde_json::Value>,
    /// Captured console/log output from `echo:` and `console.log`.
    pub stdout: String,
    /// Execution error message, if any.
    pub stderr: Option<String>,
}

/// An object listed in an S3 bucket (hex4w `StoreItem`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreItem {
    pub key: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub e_tag: Option<String>,
}

/// Standard XDB `/abc` response envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbcResponse {
    pub ok: bool,
    pub status: u16,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Hexadecimal error variants surfaced to handlers / the composition root.
#[derive(Debug, Clone)]
pub enum DomainError {
    /// Script filename not in the whitelist.
    ScriptNotAllowed(String),
    /// Script source file not found.
    ScriptNotFound(String),
    /// Malformed request.
    InvalidRequest(String),
    /// Downstream dependency rejected by the circuit breaker (HTTP 503).
    Unavailable(String),
    /// Any other failure (compile, exec, infra).
    Internal(String),
}

impl std::fmt::Display for DomainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ScriptNotAllowed(m) => write!(f, "SCRIPT_NOT_ALLOWED: {m}"),
            Self::ScriptNotFound(m) => write!(f, "SCRIPT_NOT_FOUND: {m}"),
            Self::InvalidRequest(m) => write!(f, "INVALID_REQUEST: {m}"),
            Self::Unavailable(m) => write!(f, "SERVICE_UNAVAILABLE: {m}"),
            Self::Internal(m) => write!(f, "INTERNAL_ERROR: {m}"),
        }
    }
}
impl std::error::Error for DomainError {}

/// Maps a domain error to an HTTP status code (hex4w contract).
pub fn domain_status(err: &DomainError) -> u16 {
    match err {
        DomainError::ScriptNotAllowed(_) => 403,
        DomainError::ScriptNotFound(_) => 404,
        DomainError::InvalidRequest(_) => 400,
        DomainError::Unavailable(_) => 503,
        DomainError::Internal(_) => 500,
    }
}