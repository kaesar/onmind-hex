//! Output ports (application layer) — the seams the composition root satisfies
//! with concrete infrastructure adapters (hex4w `application/ports/out`).

use crate::domain::{AbcResponse, DomainError, StoreItem};

/// XDB `/abc` access (the "Database" for ABCode scripts).
pub trait AbcPort: Send + Sync {
    /// Read a sheet/collection: `sheet(show, from, some)`.
    fn sheet(&self, show: &str, from: &str, some: &str) -> Result<AbcResponse, DomainError>;
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

/// Event fan-out (SNS/SQS/EventBridge): `services.publish(topic, payload)`.
pub trait EventPort: Send + Sync {
    fn publish(&self, topic: &str, payload: &serde_json::Value) -> Result<(), DomainError>;
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

/// Outbound email: `services.sendEmail(to, subject, body)`.
pub trait EmailPort: Send + Sync {
    fn send_email(&self, to: &str, subject: &str, body: &str) -> Result<(), DomainError>;
}