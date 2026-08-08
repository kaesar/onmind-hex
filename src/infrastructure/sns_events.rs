//! Events → SNS publisher adapter (feature `events`).
//!
//! `services.publish(topic, payload)` fans out a JSON message to an SNS topic
//! ARN. Real AWS SDK; shares the Tokio runtime with the other cloud adapters.

use std::sync::Arc;

use aws_config::BehaviorVersion;

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
    fn publish(&self, topic: &str, payload: &serde_json::Value) -> Result<(), DomainError> {
        let message = serde_json::to_string(payload)
            .map_err(|e| DomainError::Internal(format!("publish serialize: {e}")))?;
        self.rt
            .block_on(
                self.client
                    .publish()
                    .topic_arn(topic)
                    .message(message)
                    .send(),
            )
            .map(|_| ())
            .map_err(|e| DomainError::Internal(format!("sns publish {topic}: {e}")))
    }
}