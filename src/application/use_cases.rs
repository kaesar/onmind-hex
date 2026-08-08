//! Use cases (hex4w `application/usecases`) — orchestrate the script contract:
//! whitelist → load source → execute with `services`.

use crate::domain::{DomainError, ScriptResult};
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