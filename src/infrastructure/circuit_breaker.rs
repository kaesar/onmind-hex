//! Circuit breaker (pure Rust, no external dependency).
//!
//! State machine mirroring resilience4j / hex4w:
//!
//! - **CLOSED** – calls flow; failures are counted. At `failure_threshold`
//!   consecutive failures the circuit **OPENS**.
//! - **OPEN** – calls are rejected (fast-fail `503`). After `reset_timeout`
//!   the breaker moves to **HALF_OPEN**.
//! - **HALF_OPEN** – a limited number of trial calls are let through; a success
//!   closes the circuit, a failure re- **OPENS** it.
//!
//! State transitions are atomic/thread-safe.

use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::domain::DomainError;

const CLOSED: u8 = 0;
const OPEN: u8 = 1;
const HALF_OPEN: u8 = 2;

/// Circuit breaker configuration (`hex4w CircuitBreakerProperties`).
#[derive(Debug, Clone)]
pub struct CbConfig {
    pub failure_threshold: usize,
    pub reset_timeout: Duration,
    pub half_open_max: usize,
}

impl Default for CbConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            reset_timeout: Duration::from_millis(500),
            half_open_max: 1,
        }
    }
}

pub struct CircuitBreaker {
    cfg: CbConfig,
    state: AtomicU8,
    consecutive_failures: AtomicUsize,
    opened_at: Mutex<Option<Instant>>,
    half_open_inflight: AtomicUsize,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(CbConfig::default())
    }
}

impl CircuitBreaker {
    pub fn new(cfg: CbConfig) -> Self {
        Self {
            cfg,
            state: AtomicU8::new(CLOSED),
            consecutive_failures: AtomicUsize::new(0),
            opened_at: Mutex::new(None),
            half_open_inflight: AtomicUsize::new(0),
        }
    }

    pub fn state(&self) -> &'static str {
        match self.state.load(Ordering::Acquire) {
            OPEN => "open",
            HALF_OPEN => "half_open",
            _ => "closed",
        }
    }

    /// Run `f` through the breaker. Returns `Unavailable` when the circuit is
    /// open or half-open but saturated.
    pub fn run<T>(
        &self,
        f: impl FnOnce() -> Result<T, DomainError>,
    ) -> Result<T, DomainError> {
        if !self.try_acquire() {
            return Err(DomainError::Unavailable("circuit breaker open".into()));
        }
        let result = f();
        match result {
            Ok(_) => self.record_success(),
            Err(_) => self.record_failure(),
        }
        result
    }

    fn try_acquire(&self) -> bool {
        match self.state.load(Ordering::Acquire) {
            CLOSED => true,
            HALF_OPEN => {
                if self.half_open_inflight.load(Ordering::Acquire) >= self.cfg.half_open_max {
                    return false;
                }
                self.half_open_inflight.fetch_add(1, Ordering::AcqRel);
                true
            }
            // OPEN
            _ => {
                let visited = self.opened_at.lock().unwrap().is_some_and(|t| {
                    t.elapsed() >= self.cfg.reset_timeout
                });
                if visited {
                    self.state.store(HALF_OPEN, Ordering::Release);
                    true
                } else {
                    false
                }
            }
        }
    }

    fn record_success(&self) {
        match self.state.load(Ordering::Acquire) {
            HALF_OPEN => {
                self.half_open_inflight.fetch_sub(1, Ordering::AcqRel);
                self.state.store(CLOSED, Ordering::Release);
                self.consecutive_failures.store(0, Ordering::Release);
                *self.opened_at.lock().unwrap() = None;
            }
            CLOSED => {
                self.consecutive_failures.store(0, Ordering::Release);
            }
            _ => {}
        }
    }

    fn record_failure(&self) {
        match self.state.load(Ordering::Acquire) {
            CLOSED => {
                let failures = self.consecutive_failures.fetch_add(1, Ordering::AcqRel) + 1;
                if failures >= self.cfg.failure_threshold {
                    self.open();
                }
            }
            HALF_OPEN => {
                self.half_open_inflight.fetch_sub(1, Ordering::AcqRel);
                self.open();
            }
            _ => {}
        }
    }

    fn open(&self) {
        *self.opened_at.lock().unwrap() = Some(Instant::now());
        self.consecutive_failures.store(0, Ordering::Release);
        self.state.store(OPEN, Ordering::Release);
    }
}