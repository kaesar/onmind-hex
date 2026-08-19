//! Use cases (hex4w `application/usecases`) — orchestrate the script contract:
//! whitelist → load source → execute with `services`.

use std::sync::Arc;

use crate::application::ports::EventPort;
use crate::domain::{DomainError, ScriptCommand, ScriptCommandEnvelope, ScriptResult};
use crate::infrastructure::engine::AbcodeEngine;
use crate::infrastructure::source::ScriptSource;
use crate::application::script_whitelist::ScriptWhitelist;

pub struct ScriptingUseCase {
    whitelist: ScriptWhitelist,
    source: ScriptSource,
    engine: AbcodeEngine,
}

impl ScriptingUseCase {
    pub fn new(whitelist: ScriptWhitelist, source: ScriptSource, engine: AbcodeEngine) -> Self {
        Self { whitelist, source, engine }
    }

    /// Execute a whitelisted ABCode script by file name (`*.abc`).
    pub fn execute(&self, script_name: &str) -> Result<ScriptResult, DomainError> {
        self.whitelist.require_allowed(script_name)?;
        let source = self.source.load(script_name)?;
        self.engine.run(&source)
    }
}

/// Inbound event → script: executes a [`ScriptCommand`] and publishes the
/// result envelope back to the results topic (hex4w `ExecuteScriptTrait` +
/// `EventPublisherPort` in the Kafka/SQS/RabbitMQ consumers).
pub struct ScriptCommandUseCase {
    scripting: Arc<ScriptingUseCase>,
    events: Arc<dyn EventPort>,
    results_topic: String,
}

impl ScriptCommandUseCase {
    pub fn new(
        scripting: Arc<ScriptingUseCase>,
        events: Arc<dyn EventPort>,
        results_topic: String,
    ) -> Self {
        Self { scripting, events, results_topic }
    }

    /// Execute `cmd` and publish a `ScriptCommandEnvelope` on the results topic.
    /// The script result is returned; the publish failure is logged, not fatal.
    pub fn handle(&self, cmd: &ScriptCommand) -> Result<ScriptResult, DomainError> {
        let outcome = self.scripting.execute(&cmd.script);
        let (result, error) = match &outcome {
            Ok(r) => (Some(r.clone()), None),
            Err(e) => (None, Some(e.to_string())),
        };
        let envelope = ScriptCommandEnvelope {
            correlation_id: cmd.correlation_id.clone(),
            result,
            error,
        };
        let payload = serde_json::to_string(&envelope).unwrap_or_else(|_| "{}".into());
        let key = cmd.correlation_id.clone().unwrap_or_default();
        if let Err(e) = self.events.publish(&self.results_topic, &key, &payload) {
            eprintln!("[hex] script-command result publish failed: {e}");
        }
        outcome
    }
}