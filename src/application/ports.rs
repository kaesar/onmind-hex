//! Output ports (application layer) — the seams the composition root satisfies
//! with concrete infrastructure adapters (hex4w `application/ports/out`).

use serde::{Deserialize, Serialize};

use crate::domain::{AbcResponse, DomainError, Role, StoreItem};

/// XDB `/abc` request (hex4w `AbcRequest` dto-in). Maps 1:1 onto the XDB ABC
/// API payload; `where`/`sort`/`limit`/`offset` fold into `puts` over gRPC.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AbcRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub way: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub what: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub some: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub puts: Option<serde_json::Value>,
    #[serde(rename = "where", skip_serializing_if = "Option::is_none")]
    pub where_: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
}

/// Builds an `abcExec` request with hex4w's defaults (`way=sql`, `what=!`,
/// `from=xykit`).
pub fn abc_exec_request(
    what: Option<String>,
    from: Option<String>,
    some: Option<String>,
    with: Option<String>,
    puts: Option<serde_json::Value>,
) -> AbcRequest {
    AbcRequest {
        way: Some("sql".into()),
        what: Some(what.unwrap_or_else(|| "!".into())),
        from: Some(from.unwrap_or_else(|| "xykit".into())),
        some,
        show: None,
        call: None,
        with,
        puts,
        where_: None,
        sort: None,
        limit: None,
        offset: None,
    }
}

/// XDB `/abc` access (the "Database" for ABCode scripts).
pub trait AbcPort: Send + Sync {
    /// Read a sheet/collection: `sheet(show, from, some)`.
    fn sheet(&self, show: &str, from: &str, some: &str) -> Result<AbcResponse, DomainError>;
    /// Write/execute an ABC query: `exec(req)`.
    fn exec(&self, req: &AbcRequest) -> Result<AbcResponse, DomainError>;
}

/// Generic key/value cache for read-throughput adapters (e.g. the XDB decorator).
pub trait CachePort: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>, DomainError>;
    fn set(&self, key: &str, value: &str, ttl_secs: u64) -> Result<(), DomainError>;
    fn evict(&self, key: &str) -> Result<(), DomainError>;
}

/// Object store (MintStore / S3): `services.listItems`, `saveItem`, `getItem`,
/// `deleteItem` (hex4w `StorePort`).
pub trait StorePort: Send + Sync {
    fn save_item(&self, key: &str, content: &[u8]) -> Result<(), DomainError>;
    fn get_item(&self, key: &str) -> Result<Option<Vec<u8>>, DomainError>;
    fn list_items(&self, prefix: &str) -> Result<Vec<StoreItem>, DomainError>;
    fn delete_item(&self, key: &str) -> Result<(), DomainError>;
}

/// Event fan-out (SNS/SQS/EventBridge/Kafka/RabbitMQ):
/// `services.publish(topic, key, payload)` — hex4w `EventPublisherPort`.
pub trait EventPort: Send + Sync {
    fn publish(&self, topic: &str, key: &str, payload: &str) -> Result<(), DomainError>;
}

/// Role persistence (hex4w `RoleRepositoryPort`/`R2dbcRoleRepository`).
pub trait RoleRepositoryPort: Send + Sync {
    fn list(&self) -> Result<Vec<Role>, DomainError>;
    fn find(&self, id: i64) -> Result<Option<Role>, DomainError>;
    fn search_by_name(&self, name: &str) -> Result<Vec<Role>, DomainError>;
    fn save(&self, role: &Role) -> Result<Role, DomainError>;
    fn exists_by_name(&self, name: &str) -> Result<bool, DomainError>;
    fn count(&self) -> Result<i64, DomainError>;
    fn delete_by_id(&self, id: i64) -> Result<(), DomainError>;
}

/// FaaS invocation: `services.invoke`, `services.invokeAsync`.
pub trait LambdaPort: Send + Sync {
    fn invoke(
        &self,
        name: &str,
        payload: &serde_json::Value,
    ) -> Result<serde_json::Value, DomainError>;
    fn invoke_async(&self, name: &str, payload: &serde_json::Value) -> Result<(), DomainError>;
}

/// Outbound email: `services.sendEmail(to, subject, body)` (hex4w `EmailPort`).
pub trait EmailPort: Send + Sync {
    fn send_email(&self, to: &str, subject: &str, body: &str) -> Result<(), DomainError> {
        self.send_email_full(to, subject, body, None, &[])
    }
    /// hex4w `EmailPort.send(to, subject, body, from, cc)`.
    fn send_email_full(
        &self,
        to: &str,
        subject: &str,
        body: &str,
        from: Option<&str>,
        cc: &[String],
    ) -> Result<(), DomainError>;
}