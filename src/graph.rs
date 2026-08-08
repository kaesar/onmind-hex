//! Composition root: assemble the output-port adapters, the `services` facade,
//! the engine and the scripting use case once, then share them.

use std::path::PathBuf;
use std::sync::Arc;

use crate::application::ports::{AbcPort, CachePort};
use crate::application::{FacadeBuilder, ScriptWhitelist, ScriptingUseCase};
#[cfg(feature = "store")]
use crate::application::ports::StorePort;
#[cfg(feature = "events")]
use crate::application::ports::EventPort;
#[cfg(feature = "lambda")]
use crate::application::ports::LambdaPort;
#[cfg(feature = "email")]
use crate::application::ports::EmailPort;
use crate::domain::{AbcResponse, DomainError};
use crate::infrastructure::circuit_breaker::{CbConfig, CircuitBreaker};
#[cfg(feature = "abc")]
use crate::infrastructure::cb_decorator::CbAbcPort;
use crate::infrastructure::cb_decorator::CbCachePort;
use crate::infrastructure::engine::{AbcodeEngine, ServicesConfig};
use crate::infrastructure::source::ScriptSource;

pub struct Graph {
    pub scripting: ScriptingUseCase,
    abc: Option<Arc<dyn AbcPort>>,
}

impl Graph {
    /// Reads config from the environment and wires the app.
    ///
    /// Env:
    /// - `HEX_SCRIPTS_DIR`        (default `./scripts`)
    /// - `HEX_SCRIPTS_WHITELIST`  (comma-separated, default `hello.abc`)
    /// - `HEX_XDB_BASE_URL`       (feature `abc`, default `http://localhost:9990`)
    pub fn from_env() -> Self {
        let dir = std::env::var("HEX_SCRIPTS_DIR").unwrap_or_else(|_| "./scripts".into());
        let whitelist_csv =
            std::env::var("HEX_SCRIPTS_WHITELIST").unwrap_or_else(|_| "hello.abc".into());
        let whitelist = ScriptWhitelist::from_csv(&whitelist_csv);

        let source = ScriptSource::new(PathBuf::from(dir));
        let mut builder = FacadeBuilder::new();

        #[allow(unused_mut)] // mutated only when feature `abc` is enabled
        let mut abc: Option<Arc<dyn AbcPort>> = None;
        #[cfg(feature = "abc")]
        {
            let base = std::env::var("HEX_XDB_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:9990".into());
            match crate::infrastructure::XdbHttpAdapter::new(base) {
                Ok(a) => {
                    let breaker = Arc::new(CircuitBreaker::new(cb_config()));
                    let a: Arc<dyn AbcPort> = Arc::new(CbAbcPort::new(
                        Arc::new(a),
                        breaker,
                    ));
                    abc = Some(Arc::clone(&a));
                    builder = builder.with_abc(a);
                }
                Err(e) => eprintln!("[hex] xdb adapter init failed: {e}"),
            }
        }

        // Cache: in-memory for now (works without external infra), behind a
        // circuit breaker too.
        {
            let inner: Arc<dyn CachePort> =
                Arc::new(crate::infrastructure::InMemoryCache::default());
            let cb = Arc::new(CircuitBreaker::new(CbConfig::default()));
            let cache: Arc<dyn CachePort> =
                Arc::new(CbCachePort::new(inner, cb));
            builder = builder.with_cache(cache);
        }

        // Cloud adapters that need a Tokio runtime (S3, SNS, Lambda).
        #[cfg(any(feature = "store", feature = "events", feature = "lambda"))]
        {
            let rt = crate::infrastructure::cloud_runtime();

            #[cfg(feature = "store")]
            if let Ok(bucket) = std::env::var("HEX_STORE_BUCKET") {
                let store: Arc<dyn StorePort> = Arc::new(
                    crate::infrastructure::S3Store::new(bucket, Arc::clone(&rt)),
                );
                builder = builder.with_store(store);
            }

            #[cfg(feature = "events")]
            {
                let events: Arc<dyn EventPort> = Arc::new(
                    crate::infrastructure::SnsEventPublisher::new(Arc::clone(&rt)),
                );
                builder = builder.with_events(events);
            }

            #[cfg(feature = "lambda")]
            {
                let lambda: Arc<dyn LambdaPort> = Arc::new(
                    crate::infrastructure::LambdaAdapter::new(Arc::clone(&rt)),
                );
                builder = builder.with_lambda(lambda);
            }
        }

        // Email (SMTP) is blocking (no Tokio).
        #[cfg(feature = "email")]
        if let Ok(email) = crate::infrastructure::SmtpEmailSender::new_from_env() {
            let email: Arc<dyn EmailPort> = Arc::new(email);
            builder = builder.with_email(email);
        } else {
            eprintln!("[hex] smtp adapter not enabled (set HEX_SMTP_HOST/FROM)");
        }

        let services = builder.build();
        let engine = AbcodeEngine::new(Arc::new(ServicesConfig::new(services)));
        let scripting = ScriptingUseCase::new(whitelist, source, engine);

        Self { scripting, abc }
    }

    /// XDB sheet read (hex4w `GET /api/v1/xdb/sheet`).
    pub fn abc_sheet(
        &self,
        show: &str,
        from: &str,
        some: &str,
    ) -> Result<AbcResponse, DomainError> {
        match &self.abc {
            Some(p) => p.sheet(show, from, some),
            None => Err(DomainError::Internal(
                "xdb adapter not enabled (feature 'abc')".into(),
            )),
        }
    }
}

/// Circuit breaker tuning from env (hex4w `CircuitBreakerProperties`).
pub fn cb_config() -> CbConfig {
    CbConfig {
        failure_threshold: env_usize("HEX_CB_FAILURE_THRESHOLD", 5),
        reset_timeout: std::time::Duration::from_millis(env_u64("HEX_CB_RESET_MS", 500)),
        half_open_max: env_usize("HEX_CB_HALF_OPEN_MAX", 1),
    }
}

fn env_usize(k: &str, default: usize) -> usize {
    std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}
fn env_u64(k: &str, default: u64) -> u64 {
    std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}