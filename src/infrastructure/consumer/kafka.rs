//! Kafka script-command consumer (feature `events-kafka`) via rust-rdkafka's
//! blocking [`BaseConsumer`] — no Tokio needed. Mirrors hex4w
//! `KafkaEventConsumerAdapter`.
//!
//! Env:
//! - `HEX_KAFKA_BROKERS` (default `localhost:9092`)
//! - `HEX_KAFKA_GROUP`   (default `hex.script-runner`)

use std::sync::Arc;
use std::time::Duration;

use rdkafka::config::ClientConfig;
use rdkafka::consumer::{BaseConsumer, Consumer};
use rdkafka::Message;

use crate::application::ScriptCommandUseCase;
use super::parse_command;

pub fn start(use_case: &Arc<ScriptCommandUseCase>, topic: &str) -> bool {
    let brokers = std::env::var("HEX_KAFKA_BROKERS").unwrap_or_else(|_| "localhost:9092".into());
    let group = std::env::var("HEX_KAFKA_GROUP").unwrap_or_else(|_| "hex.script-runner".into());

    let consumer: BaseConsumer = match ClientConfig::new()
        .set("bootstrap.servers", &brokers)
        .set("group.id", &group)
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "true")
        .create()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[hex] kafka consumer: {e}");
            return false;
        }
    };
    if let Err(e) = consumer.subscribe(&[topic]) {
        eprintln!("[hex] kafka subscribe {topic}: {e}");
        return false;
    }

    let use_case = Arc::clone(use_case);
    std::thread::spawn(move || loop {
        match consumer.poll(Duration::from_millis(200)) {
            Some(Ok(msg)) => {
                if let Some(payload) = msg.payload() {
                    match parse_command(payload) {
                        Some(cmd) => {
                            let _ = use_case.handle(&cmd);
                        }
                        None => eprintln!(
                            "[hex] kafka: dropping unparseable message key={:?}",
                            msg.key()
                        ),
                    }
                }
            }
            Some(Err(e)) => eprintln!("[hex] kafka poll error: {e}"),
            None => {}
        }
    });

    true
}