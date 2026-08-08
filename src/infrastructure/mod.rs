//! Infrastructure adapters for the output ports.
//!
//! # feature it
//! - `default`: in-memory cache (no external deps), XDB HTTP via `reqwest`
//!   (`abc`), the rest of the `services.*` surface returns `Unsupported`.
//! - Opt-in cloud adapters mount into the same `services.*` facade as features:
//!   `store` → S3, `events` → SNS, `lambda` → Lambda, `email` → SMTP.

pub mod source;
pub mod engine;
pub mod circuit_breaker;
pub mod cb_decorator;

#[cfg(feature = "abc")]
mod abc_http;

#[cfg(feature = "store")]
mod s3_store;
#[cfg(feature = "store")]
pub use s3_store::S3Store;

#[cfg(feature = "events")]
mod sns_events;
#[cfg(feature = "events")]
pub use sns_events::SnsEventPublisher;

#[cfg(feature = "lambda")]
mod lambda_client;
#[cfg(feature = "lambda")]
pub use lambda_client::LambdaAdapter;

#[cfg(feature = "email")]
mod smtp_email;
#[cfg(feature = "email")]
pub use smtp_email::SmtpEmailSender;

/// Shared synchronous-to-async Tokio runtime for the cloud adapters.
#[cfg(any(feature = "store", feature = "events", feature = "lambda", feature = "email"))]
pub fn cloud_runtime() -> std::sync::Arc<tokio::runtime::Runtime> {
    std::sync::Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio multi-thread runtime"),
    )
}

/// In-memory cache (used when `cache`/redis is not configured).
pub struct InMemoryCache {
    inner: std::sync::Mutex<std::collections::HashMap<String, (String, u64, std::time::Instant)>>,
}

impl Default for InMemoryCache {
    fn default() -> Self {
        Self {
            inner: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl crate::application::ports::CachePort for InMemoryCache {
    fn get(&self, key: &str) -> Result<Option<String>, crate::domain::DomainError> {
        let now = std::time::Instant::now();
        let mut m = self.inner.lock().unwrap();
        match m.get(key) {
            Some((v, ttl, at)) if now.duration_since(*at).as_secs() < *ttl => Ok(Some(v.clone())),
            Some(_) => {
                m.remove(key);
                Ok(None)
            }
            None => Ok(None),
        }
    }
    fn set(
        &self,
        key: &str,
        value: &str,
        ttl_secs: u64,
    ) -> Result<(), crate::domain::DomainError> {
        let mut m = self.inner.lock().unwrap();
        m.insert(key.to_string(), (value.to_string(), ttl_secs, std::time::Instant::now()));
        Ok(())
    }
    fn evict(&self, key: &str) -> Result<(), crate::domain::DomainError> {
        self.inner.lock().unwrap().remove(key);
        Ok(())
    }
}

/// XDB `/abc` HTTP adapter (feature `abc`).
#[cfg(feature = "abc")]
pub use abc_http::XdbHttpAdapter;