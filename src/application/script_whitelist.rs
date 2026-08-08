//! Whitelist of executable script file names (hex4w `ScriptWhitelist`).
//!
//! This is the primary security control: only scripts listed here may be
//! executed by the scripts API. Configured via the `HEX_SCRIPTS_WHITELIST`
//! environment variable (comma-separated) or code.

use crate::domain::DomainError;

#[derive(Debug, Clone)]
pub struct ScriptWhitelist {
    allowed: Vec<String>,
}

impl ScriptWhitelist {
    /// Build from a comma-separated list of names (e.g. `"hello.abc,services.abc"`).
    pub fn from_csv(csv: &str) -> Self {
        let allowed = csv
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        Self { allowed }
    }

    /// `true` if `name` is allowed (case-sensitive exact match).
    pub fn is_allowed(&self, name: &str) -> bool {
        self.allowed.iter().any(|a| a == name)
    }

    /// Ensure `name` is whitelisted; returns `ScriptNotAllowed` otherwise.
    pub fn require_allowed(&self, name: &str) -> Result<(), DomainError> {
        if self.is_allowed(name) {
            Ok(())
        } else {
            Err(DomainError::ScriptNotAllowed(format!(
                "'{name}' not allowed. Allowed: {}",
                self.allowed.join(", ")
            )))
        }
    }

    pub fn allowed(&self) -> &[String] {
        &self.allowed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_parsing_and_matching() {
        let w = ScriptWhitelist::from_csv("hello.abc, services.abc ");
        assert!(w.is_allowed("hello.abc"));
        assert!(w.is_allowed("services.abc"));
        assert!(!w.is_allowed("x.abc"));
        assert!(w.require_allowed("hello.abc").is_ok());
        assert!(matches!(
            w.require_allowed("x.abc"),
            Err(DomainError::ScriptNotAllowed(_))
        ));
    }
}