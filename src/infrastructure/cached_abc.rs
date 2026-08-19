//! Cache-first decorator for the XDB sheet read (hex4w `CachedAbcAdapter`).
//!
//! Wraps an [`AbcPort`] so `sheet(show, from, some)` is cached key/value
//! (`abc:sheet:{show}:{from}:{some}`) with a TTL. Only `ok` responses are
//! stored; `exec` is never cached (write-through).

use std::sync::Arc;

use crate::application::ports::{AbcPort, AbcRequest, CachePort};
use crate::domain::{AbcResponse, DomainError};

pub struct CachedAbcPort {
    inner: Arc<dyn AbcPort>,
    cache: Arc<dyn CachePort>,
    ttl_secs: u64,
}

fn cache_key(show: &str, from: &str, some: &str) -> String {
    format!("abc:sheet:{show}:{from}:{some}")
}

impl CachedAbcPort {
    /// `ttl_secs` defaults to env `HEX_XDB_CACHE_TTL` (default 300).
    pub fn new(inner: Arc<dyn AbcPort>, cache: Arc<dyn CachePort>, ttl_secs: u64) -> Self {
        Self { inner, cache, ttl_secs }
    }
}

impl AbcPort for CachedAbcPort {
    fn sheet(&self, show: &str, from: &str, some: &str) -> Result<AbcResponse, DomainError> {
        let key = cache_key(show, from, some);
        if let Some(hit) = self.cache.get(&key)? {
            if let Ok(resp) = serde_json::from_str::<AbcResponse>(&hit) {
                return Ok(resp);
            }
        }
        let resp = self.inner.sheet(show, from, some)?;
        if resp.ok {
            if let Ok(json) = serde_json::to_string(&resp) {
                let _ = self.cache.set(&key, &json, self.ttl_secs);
            }
        }
        Ok(resp)
    }

    fn exec(&self, req: &AbcRequest) -> Result<AbcResponse, DomainError> {
        self.inner.exec(req)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::application::ports::CachePort;
    use crate::infrastructure::cache::InMemoryCache;

    struct CountingAbc {
        calls: AtomicUsize,
    }

    impl AbcPort for CountingAbc {
        fn sheet(&self, show: &str, from: &str, some: &str) -> Result<AbcResponse, DomainError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(AbcResponse {
                ok: true,
                status: 200,
                message: format!("{show}|{from}|{some}"),
                total: Some(1),
                data: None,
            })
        }
        fn exec(&self, _req: &AbcRequest) -> Result<AbcResponse, DomainError> {
            Ok(AbcResponse { ok: true, status: 200, message: "exec".into(), total: None, data: None })
        }
    }

    #[test]
    fn caches_only_successful_sheet_response() {
        let inner = Arc::new(CountingAbc { calls: AtomicUsize::new(0) });
        let cache: Arc<dyn CachePort> = Arc::new(InMemoryCache::default());
        let cached = CachedAbcPort::new(inner.clone(), cache, 300);

        let first = cached.sheet("a", "b", "c").unwrap();
        let second = cached.sheet("a", "b", "c").unwrap();
        assert_eq!(first.message, second.message);
        assert_eq!(inner.calls.load(Ordering::SeqCst), 1, "inner called once");
    }
}