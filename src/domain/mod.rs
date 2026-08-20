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
    #[serde(rename = "lastModified", default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    #[serde(rename = "eTag", default, skip_serializing_if = "Option::is_none")]
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

/// Role aggregate (hex4w `Role`/`RoleEntity`), persisted by the `db` adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Role {
    pub id: i64,
    pub name: String,
    #[serde(rename = "createdAt", default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

/// Inbound event command `{ script, correlationId }` (hex4w `KafkaScriptCommand`).
#[derive(Debug, Clone, Deserialize)]
pub struct ScriptCommand {
    pub script: String,
    #[serde(rename = "correlationId", default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
}

/// Result published back to `hex4w.script.results` (hex4w `ScriptResultEnvelope`).
#[derive(Debug, Clone, Serialize)]
pub struct ScriptCommandEnvelope {
    #[serde(rename = "correlationId")]
    pub correlation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<ScriptResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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
    /// Role name already exists (hex4w `DuplicateRoleException`, HTTP 409).
    Duplicate(String),
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
            Self::Duplicate(m) => write!(f, "DUPLICATE_ROLE: {m}"),
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
        DomainError::Duplicate(_) => 409,
        DomainError::Unavailable(_) => 503,
        DomainError::Internal(_) => 500,
    }
}

/// hex4w `RoleService` name validation: trimmed, 2..=50 chars, charset
/// `[a-zA-Z0-9_ -]`, reserved names and system prefixes rejected.
pub fn validate_role_name(name: &str) -> Result<String, DomainError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(DomainError::InvalidRequest("Role name cannot be null or empty".into()));
    }
    if trimmed.len() < 2 {
        return Err(DomainError::InvalidRequest("Role name must be at least 2 characters long".into()));
    }
    if trimmed.len() > 50 {
        return Err(DomainError::InvalidRequest("Role name cannot exceed 50 characters".into()));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ' ' || c == '-' || c == '\t')
    {
        return Err(DomainError::InvalidRequest(
            "Role name can only contain letters, numbers, spaces, hyphens and underscores".into(),
        ));
    }
    let upper = trimmed.to_uppercase();
    for reserved in ["SYSTEM", "ROOT", "NULL", "UNDEFINED"] {
        if upper.contains(reserved) {
            return Err(DomainError::InvalidRequest(
                format!("Role name '{trimmed}' is reserved and cannot be used"),
            ));
        }
    }
    if upper.starts_with("SYS_") || upper.starts_with("INTERNAL_") {
        return Err(DomainError::InvalidRequest(
            "Role name cannot start with system prefixes (SYS_, INTERNAL_)".into(),
        ));
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_name_rules() {
        assert_eq!(validate_role_name("  admin  ").unwrap(), "admin");
        assert!(validate_role_name("a").is_err());
        assert_eq!(validate_role_name(&"x".repeat(51)).unwrap_err().to_string(), "INVALID_REQUEST: Role name cannot exceed 50 characters");
        assert!(validate_role_name("va l!d?").is_err());
        assert!(validate_role_name("SYSTEM X").is_err());
        assert!(validate_role_name("SYS_PRE").is_err());
        assert!(validate_role_name("INTERNAL__x").is_err());
        assert!(validate_role_name("fine-role_2").is_ok());
    }

    #[test]
    fn duplicate_maps_to_409() {
        assert_eq!(domain_status(&DomainError::Duplicate("ADMIN".into())), 409);
        assert_eq!(domain_status(&DomainError::InvalidRequest("x".into())), 400);
    }

    #[test]
    fn store_and_role_json_use_hex4w_field_names() {
        let item = StoreItem { key: "k".into(), size: 1, last_modified: Some("t".into()), e_tag: Some("e".into()) };
        let v: serde_json::Value = serde_json::to_value(item).unwrap();
        assert_eq!(v["lastModified"], "t");
        assert_eq!(v["eTag"], "e");
        assert!(v.get("last_modified").is_none());

        let role = Role { id: 1, name: "x".into(), created_at: Some("now".into()) };
        let v: serde_json::Value = serde_json::to_value(role).unwrap();
        assert_eq!(v["createdAt"], "now");
        assert!(v.get("created_at").is_none());
    }
}