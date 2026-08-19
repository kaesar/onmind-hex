//! Events → SNS publisher adapter (feature `events`).
//!
//! `services.publish(topic, key, payload)` fans out a JSON message to an SNS
//! topic ARN, carrying `key` as a `key` message attribute. Real AWS SDK; shares
//! the Tokio runtime with the other cloud adapters.

use std::sync::Arc;

use aws_config::BehaviorVersion;
use aws_sdk_sns::types::MessageAttributeValue;

use crate::domain::DomainError;

pub struct SnsEventPublisher {
    client: aws_sdk_sns::Client,
    rt: Arc<tokio::runtime::Runtime>,
}

impl SnsEventPublisher {
    pub fn new(rt: Arc<tokio::runtime::Runtime>) -> Self {
        let cfg = rt.block_on(aws_config::defaults(BehaviorVersion::latest()).load());
        Self {
            client: aws_sdk_sns::Client::new(&cfg),
            rt,
        }
    }
}

impl crate::application::ports::EventPort for SnsEventPublisher {
    fn publish(&self, topic: &str, key: &str, payload: &str) -> Result<(), DomainError> {
        let mut req = self
            .client
            .publish()
            .topic_arn(topic)
            .message(payload.to_string());
        if !key.is_empty() {
            let attr = MessageAttributeValue::builder()
                .data_type("String")
                .string_value(key)
                .build()
                .expect("valid message attribute");
            req = req.message_attributes("key", attr);
        }
        self.rt
            .block_on(req.send())
            .map(|_| ())
            .map_err(|e| DomainError::Internal(format!("sns publish {topic}: {e}")))
    }
}