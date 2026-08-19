//! Cache adapters for [`CachePort`] (hex4w `RedisCacheAdapter`).
//!
//! - `InMemoryCache`: process-local map with TTL — works with zero external
//!   infra (used as the fallback when Redis is unreachable).
//! - `RedisCache`: real Redis (blocking client, `SETEX`/`GET`/`DEL`).
//!   Configured with `HEX_REDIS_URL` (default `redis://127.0.0.1:6379`).

use std::sync::Mutex;

use crate::application::ports::CachePort;
use crate::domain::DomainError;

/// In-memory cache (used when Redis is not configured / unreachable).
pub struct InMemoryCache {
    inner: Mutex<std::collections::HashMap<String, (String, u64, std::time::Instant)>>,
}

impl Default for InMemoryCache {
    fn default() -> Self {
        Self {
            inner: Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl CachePort for InMemoryCache {
    fn get(&self, key: &str) -> Result<Option<String>, DomainError> {
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
    fn set(&self, key: &str, value: &str, ttl_secs: u64) -> Result<(), DomainError> {
        let mut m = self.inner.lock().unwrap();
        m.insert(key.to_string(), (value.to_string(), ttl_secs, std::time::Instant::now()));
        Ok(())
    }
    fn evict(&self, key: &str) -> Result<(), DomainError> {
        self.inner.lock().unwrap().remove(key);
        Ok(())
    }
}

/// Real Redis adapter (feature `cache`). The connection is created once at
/// startup; `redis::Connection` is `Send` but not `Sync`, hence the mutex.
#[cfg(feature = "cache")]
pub struct RedisCache {
    conn: Mutex<redis::Connection>,
}

#[cfg(feature = "cache")]
impl RedisCache {
    /// Connect to Redis; fails (caller falls back to in-memory) on any error.
    pub fn connect(url: &str) -> Result<Self, DomainError> {
        let client = redis::Client::open(url)
            .map_err(|e| DomainError::Internal(format!("redis open {url}: {e}")))?;
        let conn = client
            .get_connection()
            .map_err(|e| DomainError::Internal(format!("redis connect {url}: {e}")))?;
        Ok(Self { conn: Mutex::new(conn) })
    }
}

#[cfg(feature = "cache")]
impl CachePort for RedisCache {
    fn get(&self, key: &str) -> Result<Option<String>, DomainError> {
        use redis::Commands;
        let mut conn = self.conn.lock().unwrap();
        conn.get::<_, Option<String>>(key)
            .map_err(|e| DomainError::Internal(format!("redis get {key}: {e}")))
    }
    fn set(&self, key: &str, value: &str, ttl_secs: u64) -> Result<(), DomainError> {
        use redis::Commands;
        let mut conn = self.conn.lock().unwrap();
        conn.set_ex::<_, _, ()>(key, value, ttl_secs)
            .map_err(|e| DomainError::Internal(format!("redis set {key}: {e}")))
    }
    fn evict(&self, key: &str) -> Result<(), DomainError> {
        use redis::Commands;
        let mut conn = self.conn.lock().unwrap();
        conn.del::<_, ()>(key)
            .map_err(|e| DomainError::Internal(format!("redis del {key}: {e}")))
    }
}