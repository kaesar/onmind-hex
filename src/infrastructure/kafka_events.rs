//! Events → Kafka publisher adapter (feature `events-kafka`), using the
//! synchronous `BaseProducer` from rust-rdkafka 0.39. No Tokio required.
//!
//! `services.publish(topic, key, payload)` produces a message with `key` set
//! (so ordering per key is honoured). Flushed before returning so callers see
//! a best-effort success only once the broker acked.
//!
//! Env:
//! - `HEX_KAFKA_BROKERS` (default `localhost:9092`)

use rdkafka::config::ClientConfig;
use rdkafka::producer::{BaseProducer, BaseRecord, Producer};

use crate::domain::DomainError;

pub struct KafkaEventPublisher {
    producer: BaseProducer,
}

impl KafkaEventPublisher {
    pub fn new() -> Result<Self, DomainError> {
        let brokers =
            std::env::var("HEX_KAFKA_BROKERS").unwrap_or_else(|_| "localhost:9092".into());
        let producer: BaseProducer = ClientConfig::new()
            .set("bootstrap.servers", &brokers)
            .set("message.timeout.ms", "5000")
            .create()
            .map_err(|e| DomainError::Internal(format!("kafka producer: {e}")))?;
        Ok(Self { producer })
    }
}

impl crate::application::ports::EventPort for KafkaEventPublisher {
    fn publish(&self, topic: &str, key: &str, payload: &str) -> Result<(), DomainError> {
        let mut record = BaseRecord::to(topic).payload(payload);
        if !key.is_empty() {
            record = record.key(key);
        }
        self.producer
            .send(record)
            .map_err(|(e, _)| DomainError::Internal(format!("kafka send {topic}: {e}")))?;
        self.producer
            .flush(std::time::Duration::from_secs(5))
            .map_err(|e| DomainError::Internal(format!("kafka flush {topic}: {e}")))?;
        Ok(())
    }
}