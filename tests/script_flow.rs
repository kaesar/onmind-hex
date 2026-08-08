//! End-to-end test: whitelist → load ABCode → compile → execute with `services`.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use hex::application::ports::CachePort;
use hex::application::{FacadeBuilder, ScriptWhitelist, ScriptingUseCase};
use hex::infrastructure::engine::{AbcodeEngine, ServicesConfig};
use hex::infrastructure::source::ScriptSource;
use hex::infrastructure::InMemoryCache;

const SCRIPT: &str = r#"goal: any
fun: handler()
  val: cached = services.cacheGet("k")
  run: services.cacheSet("hello-flag", "1", 300)
  pass: {"hello": "hex", "cached": cached}
run: handler()"#;

fn temp_script_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hex_it_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("hello.abc");
    fs::write(&path, SCRIPT).unwrap();
    dir
}

#[test]
fn script_executes_with_services() {
    let dir = temp_script_dir();
    let whitelist = ScriptWhitelist::from_csv("hello.abc");
    let source = ScriptSource::new(dir);

    let cache: Arc<dyn CachePort> = Arc::new(InMemoryCache::default());
    let services = FacadeBuilder::new().with_cache(cache).build();
    let engine = AbcodeEngine::new(Arc::new(ServicesConfig::new(services)));
    let scripting = ScriptingUseCase::new(whitelist, source, engine);

    let result = scripting.execute("hello.abc").expect("script runs");
    assert_eq!(result.stderr, None);
    let value = result.value.expect("a completion value");
    assert_eq!(value["cached"], serde_json::Value::Null);
}

#[test]
fn non_whitelisted_script_is_rejected() {
    let dir = temp_script_dir();
    let whitelist = ScriptWhitelist::from_csv("hello.abc");
    let source = ScriptSource::new(dir);
    let engine = AbcodeEngine::new(Arc::new(ServicesConfig::new(vec![])));
    let scripting = ScriptingUseCase::new(whitelist, source, engine);

    let err = scripting.execute("other.abc").unwrap_err();
    assert!(matches!(err, hex::domain::DomainError::ScriptNotAllowed(_)));
}