//! Loads ABCode script source from a scripts directory (`scripts/*.abc`).
//! Mirrors hex4w `ScriptSourcePort`/`ClasspathScriptSourceAdapter`:
//! defense-in-depth path-traversal rejection, then read.

use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::DomainError;

#[derive(Clone)]
pub struct ScriptSource {
    dir: PathBuf,
}

impl ScriptSource {
    /// `dir` defaults to `./scripts` relative to the process cwd.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Load the ABCode source for `name` (expected `*.abc`).
    pub fn load(&self, name: &str) -> Result<String, DomainError> {
        // Defense in depth: reject traversal even if the whitelist is bypassed.
        if name.is_empty()
            || name.contains("..")
            || name.contains('/')
            || name.contains('\\')
        {
            return Err(DomainError::ScriptNotFound(format!("invalid script name: {name}")));
        }

        let path = Path::new(&self.dir).join(name);
        if !path.exists() {
            return Err(DomainError::ScriptNotFound(format!("{name} not found in scripts/")));
        }
        fs::read_to_string(&path)
            .map_err(|e| DomainError::Internal(format!("read {name}: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal() {
        let src = ScriptSource::new(std::env::temp_dir());
        assert!(src.load("../Cargo.toml").is_err());
        assert!(src.load("a/b.abc").is_err());
        assert!(src.load("..").is_err());
    }
}