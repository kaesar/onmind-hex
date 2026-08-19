//! Inbound event → script consumers (hex4w `*EventConsumerAdapter`).
//!
//! A single dispatcher is chosen with `HEX_CONSUMER_TYPE` (`sqs`, `kafka`,
//! `rabbit`, `redis`). Each consumer listens on the command topic
//! (`HEX_SCRIPT_COMMANDS_TOPIC`, default `hex4w.script.commands`), parses a
//! [`ScriptCommand`] and delegates to [`ScriptCommandUseCase`], which publishes
//! the result envelope back to the results topic.

use std::sync::Arc;

use crate::application::ScriptCommandUseCase;
use crate::domain::ScriptCommand;
use crate::graph::Graph;

#[cfg(feature = "events-sqs")]
pub mod sqs;
#[cfg(feature = "events-kafka")]
pub mod kafka;
#[cfg(feature = "events-rabbit")]
pub mod rabbit;
#[cfg(feature = "cache")]
pub mod redis;

pub fn commands_topic() -> String {
    std::env::var("HEX_SCRIPT_COMMANDS_TOPIC")
        .unwrap_or_else(|_| "hex4w.script.commands".into())
}

/// Parse a command message body (`{ "script": ..., "correlationId": ... }`).
pub fn parse_command(body: &[u8]) -> Option<ScriptCommand> {
    let text = std::str::from_utf8(body).ok()?;
    if text.trim().is_empty() {
        return None;
    }
    serde_json::from_str::<ScriptCommand>(text).ok()
}

/// Start the consumer selected by `HEX_CONSUMER_TYPE`, if a script-command use
/// case (i.e. an event publisher) is wired.
pub fn start_consumers(graph: &Arc<Graph>) {
    let Some(use_case) = graph.script_commands() else {
        return;
    };
    let ty = std::env::var("HEX_CONSUMER_TYPE").unwrap_or_default();
    let topic = commands_topic();

    let ok = match ty.as_str() {
        "sqs" => start_sqs(&use_case, &topic),
        "kafka" => start_kafka(&use_case, &topic),
        "rabbit" => start_rabbit(&use_case, &topic),
        "redis" => start_redis(&use_case, &topic),
        "" => return,
        other => {
            eprintln!("[hex] unknown HEX_CONSUMER_TYPE '{other}'");
            return;
        }
    };

    match ok {
        true => println!("- consumer: {ty} (commands → {topic})"),
        false => eprintln!("[hex] consumer '{ty}' failed to start"),
    }
}

#[cfg(feature = "events-sqs")]
fn start_sqs(uc: &Arc<ScriptCommandUseCase>, topic: &str) -> bool {
    sqs::start(uc, topic)
}
#[cfg(not(feature = "events-sqs"))]
fn start_sqs(_: &Arc<ScriptCommandUseCase>, _: &str) -> bool {
    eprintln!("[hex] HEX_CONSUMER_TYPE=sqs but feature 'events-sqs' is off");
    false
}

#[cfg(feature = "events-kafka")]
fn start_kafka(uc: &Arc<ScriptCommandUseCase>, topic: &str) -> bool {
    kafka::start(uc, topic)
}
#[cfg(not(feature = "events-kafka"))]
fn start_kafka(_: &Arc<ScriptCommandUseCase>, _: &str) -> bool {
    eprintln!("[hex] HEX_CONSUMER_TYPE=kafka but feature 'events-kafka' is off");
    false
}

#[cfg(feature = "events-rabbit")]
fn start_rabbit(uc: &Arc<ScriptCommandUseCase>, topic: &str) -> bool {
    rabbit::start(uc, topic)
}
#[cfg(not(feature = "events-rabbit"))]
fn start_rabbit(_: &Arc<ScriptCommandUseCase>, _: &str) -> bool {
    eprintln!("[hex] HEX_CONSUMER_TYPE=rabbit but feature 'events-rabbit' is off");
    false
}

#[cfg(feature = "cache")]
fn start_redis(uc: &Arc<ScriptCommandUseCase>, topic: &str) -> bool {
    redis::start(uc, topic)
}
#[cfg(not(feature = "cache"))]
fn start_redis(_: &Arc<ScriptCommandUseCase>, _: &str) -> bool {
    eprintln!("[hex] HEX_CONSUMER_TYPE=redis but feature 'cache' is off");
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_command_message() {
        let cmd = parse_command(br#"{"script":"hello.abc","correlationId":"c-1"}"#).unwrap();
        assert_eq!(cmd.script, "hello.abc");
        assert_eq!(cmd.correlation_id.as_deref(), Some("c-1"));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_command(b"not json").is_none());
        assert!(parse_command(b"").is_none());
    }
}