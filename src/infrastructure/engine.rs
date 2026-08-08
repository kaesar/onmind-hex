//! ABCode execution engine: compiles ABCode to JS (abcodelib) and runs it with
//! a host `services` object injected into the Boa context.

use abcodelib::{ExecuteResult, HostService};
use std::sync::{mpsc, Arc};
use std::thread;

use crate::domain::{DomainError, ScriptResult};

/// Holds the host callbacks to expose as the global `services` object.
pub struct ServicesConfig {
    services: Vec<HostService>,
}

impl ServicesConfig {
    pub fn new(services: Vec<HostService>) -> Self {
        Self { services }
    }
}

/// Compiles + executes an ABCode script in-process (thread-safe).
pub struct AbcodeEngine {
    services: Arc<ServicesConfig>,
}

impl AbcodeEngine {
    pub fn new(services: Arc<ServicesConfig>) -> Self {
        Self { services }
    }

    /// Run ABCode `source`, returning value + captured logs.
    pub fn run(&self, source: &str) -> Result<ScriptResult, DomainError> {
        let (tx, rx) = mpsc::channel();
        let source = source.to_string();
        let config = Arc::clone(&self.services);

        // Thread with a generous stack: Boa compilation + execution can recurse.
        thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                let out = compile_and_run(&source, &config.services);
                let _ = tx.send(out);
            })
            .map_err(|e| DomainError::Internal(format!("spawn: {e}")))?;

        rx.recv()
            .map_err(|e| DomainError::Internal(format!("recv: {e}")))?
    }
}

fn compile_and_run(source: &str, services: &[HostService]) -> Result<ScriptResult, DomainError> {
    let compiled = abcodelib::compile(1, source, "*")
        .map_err(|e| DomainError::Internal(format!("compile: {e}")))?;

    let exec: ExecuteResult = abcodelib::execute_js_with_services(&compiled.code, services)
        .map_err(|e| DomainError::Internal(format!("exec: {e}")))?;

    let value = match exec.value_json {
        Some(json_str) => match serde_json::from_str(&json_str) {
            Ok(v) => Some(v),
            Err(_) => Some(serde_json::Value::String(json_str)),
        },
        None => None,
    };

    Ok(ScriptResult {
        value,
        stdout: exec.logs,
        stderr: None,
    })
}