//! Events → RabbitMQ publisher adapter (feature `events-rabbit`), via lapin.
//!
//! `services.publish(topic, key, payload)` publishes to a Topic exchange with
//! `key` as the routing key (hex4w `RabbitMQEventPublisherAdapter`). Requires
//! a Tokio runtime; each publish is driven with `block_on`.
//!
//! Env:
//! - `HEX_RABBIT_URL`       (default `amqp://guest:guest@localhost:5672`)
//! - `HEX_RABBIT_EXCHANGE`  (default `hex.events`)

use std::sync::{Arc, Mutex};

use lapin::options::{BasicPublishOptions, ExchangeDeclareOptions};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind};

use crate::domain::DomainError;

pub struct RabbitEventPublisher {
    rt: Arc<tokio::runtime::Runtime>,
    channel: Mutex<Channel>,
    exchange: String,
}

impl RabbitEventPublisher {
    pub fn new(rt: Arc<tokio::runtime::Runtime>) -> Result<Self, DomainError> {
        let url = std::env::var("HEX_RABBIT_URL")
            .unwrap_or_else(|_| "amqp://guest:guest@localhost:5672".into());
        let exchange = std::env::var("HEX_RABBIT_EXCHANGE").unwrap_or_else(|_| "hex.events".into());
        let declared = exchange.clone();

        let created: Result<Channel, lapin::Error> = rt.block_on(async move {
            let conn = Connection::connect(&url, ConnectionProperties::default()).await?;
            let channel = conn.create_channel().await?;
            channel
                .exchange_declare(
                    &declared,
                    ExchangeKind::Topic,
                    ExchangeDeclareOptions::default(),
                    FieldTable::default(),
                )
                .await?;
            Ok::<_, lapin::Error>(channel)
        });
        let channel = created
            .map_err(|e| DomainError::Internal(format!("rabbit connect: {e}")))?;

        Ok(Self { rt, channel: Mutex::new(channel), exchange })
    }
}

impl crate::application::ports::EventPort for RabbitEventPublisher {
    fn publish(&self, topic: &str, key: &str, payload: &str) -> Result<(), DomainError> {
        let exchange = if topic.is_empty() { &self.exchange } else { topic };
        let channel = self.channel.lock().unwrap();
        self.rt
            .block_on(channel.basic_publish(
                exchange,
                key,
                BasicPublishOptions::default(),
                payload.as_bytes(),
                BasicProperties::default(),
            ))
            .map_err(|e| DomainError::Internal(format!("rabbit publish {exchange}/{key}: {e}")))?;
        Ok(())
    }
}