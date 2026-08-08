//! Circuit-breaker wrappers for the output ports (like hex4w's `failsafe`
//! decorators around `ScriptServicesFacade` outbound calls).

use std::sync::Arc;

use crate::application::ports::{AbcPort, CachePort};
use crate::domain::{AbcResponse, DomainError};
use crate::infrastructure::circuit_breaker::CircuitBreaker;

/// Decorates an [`AbcPort`] (XDB `/abc`) so failing calls open the circuit and
/// rejected calls fast-fail with `503 Unavailable`.
pub struct CbAbcPort {
    inner: Arc<dyn AbcPort>,
    cb: Arc<CircuitBreaker>,
}

impl CbAbcPort {
    pub fn new(inner: Arc<dyn AbcPort>, cb: Arc<CircuitBreaker>) -> Self {
        Self { inner, cb }
    }
}

impl AbcPort for CbAbcPort {
    fn sheet(
        &self,
        show: &str,
        from: &str,
        some: &str,
    ) -> Result<AbcResponse, DomainError> {
        let inner = Arc::clone(&self.inner);
        let show = show.to_string();
        let from = from.to_string();
        let some = some.to_string();
        self.cb.run(move || inner.sheet(&show, &from, &some))
    }
}

/// Cache put t is route; breaks on repeated cache failures.
pub struct CbCachePort {
    inner: Arc<dyn CachePort>,
    cb: Arc<CircuitBreaker>,
}

impl CbCachePort {
    pub fn new(inner: Arc<dyn CachePort>, cb: Arc<CircuitBreaker>) -> Self {
        Self { inner, cb }
    }
}

impl CachePort for CbCachePort {
    fn get(&self, key: &str) -> Result<Option<String>, DomainError> {
        let inner = Arc::clone(&self.inner);
        let key = key.to_string();
        self.cb.run(move || inner.get(&key))
    }
    fn set(&self, key: &str, value: &str, ttl: u64) -> Result<(), DomainError> {
        let inner = Arc::clone(&self.inner);
        let key = key.to_string();
        let value = value.to_string();
        self.cb.run(move || inner.set(&key, &value, ttl))
    }
    fn evict(&self, key: &str) -> Result<(), DomainError> {
        let inner = Arc::clone(&self.inner);
        let key = key.to_string();
        self.cb.run(move || inner.evict(&key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::circuit_breaker::CbConfig;
    use std::time::Duration;

    struct Flaky;

    impl AbcPort for Flaky {
        fn sheet(
            &self,
            _show: &str,
            _from: &str,
            _some: &str,
        ) -> Result<AbcResponse, DomainError> {
            Err(DomainError::Internal("boom".into()))
        }
    }

    #[test]
    fn opens_after_threshold_then_rejects() {
        let cb = Arc::new(CircuitBreaker::new(CbConfig {
            failure_threshold: 2,
            reset_timeout: Duration::from_millis(50),
            half_open_max: 1,
        }));
        let wrapped = CbAbcPort::new(Arc::new(Flaky), Arc::clone(&cb));

        assert!(matches!(
            wrapped.sheet("a", "b", "c"),
            Err(DomainError::Internal(_))
        ));
        assert_eq!(cb.state(), "closed");
        assert!(matches!(
            wrapped.sheet("a", "b", "c"),
            Err(DomainError::Internal(_))
        ));
        assert_eq!(cb.state(), "open");
        assert!(matches!(
            wrapped.sheet("a", "b", "c"),
            Err(DomainError::Unavailable(_))
        ));
    }
}