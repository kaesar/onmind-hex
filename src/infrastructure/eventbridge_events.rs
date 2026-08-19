//! Events → Amazon EventBridge publisher adapter (feature `events-eventbridge`).
//!
//! `services.publish(topic, key, payload)` puts a custom event on a bus:
//! source=`hex`, detail-type=`key`, detail=`payload`.
//!
//! Env:
//! - `HEX_EVENTBRIDGE_BUS` (default `default`)

use std::sync::Arc;

use aws_config::BehaviorVersion;
use aws_sdk_eventbridge::types::PutEventsRequestEntry;

use crate::domain::DomainError;

pub struct EventBridgeEventPublisher {
    client: aws_sdk_eventbridge::Client,
    bus: String,
    rt: Arc<tokio::runtime::Runtime>,
}

impl EventBridgeEventPublisher {
    pub fn new(runtime: Arc<tokio::runtime::Runtime>) -> Self {
        let cfg = runtime.block_on(aws_config::defaults(BehaviorVersion::latest()).load());
        Self {
            client: aws_sdk_eventbridge::Client::new(&cfg),
            bus: std::env::var("HEX_EVENTBRIDGE_BUS").unwrap_or_else(|_| "default".into()),
            rt: runtime,
        }
    }
}

impl crate::application::ports::EventPort for EventBridgeEventPublisher {
    fn publish(&self, topic: &str, key: &str, payload: &str) -> Result<(), DomainError> {
        let bus = if topic.is_empty() { &self.bus } else { topic };
        let entry = PutEventsRequestEntry::builder()
            .event_bus_name(bus)
            .source("hex")
            .detail_type(key)
            .detail(payload)
            .build();
        self.rt
            .block_on(self.client.put_events().entries(entry).send())
            .map(|_| ())
            .map_err(|e| DomainError::Internal(format!("eventbridge publish {bus}: {e}")))
    }
}