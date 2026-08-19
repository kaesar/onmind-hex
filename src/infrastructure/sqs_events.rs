//! Events → SQS publisher adapter (feature `events-sqs`).
//!
//! `services.publish(topic, key, payload)` sends a message to the SQS queue
//! URL given as `topic`, attaching `key` as a message attribute. The consumer
//! counterpart polls the same queue (see `consumer::sqs_consumer`).
//!
//! Env: `HEX_SQS_QUEUE_URL`.

use std::sync::Arc;

use aws_config::BehaviorVersion;
use aws_sdk_sqs::types::MessageAttributeValue;

use crate::domain::DomainError;

pub struct SqsEventPublisher {
    client: aws_sdk_sqs::Client,
    queue_url: String,
    rt: Arc<tokio::runtime::Runtime>,
}

impl SqsEventPublisher {
    pub fn new(runtime: Arc<tokio::runtime::Runtime>) -> Result<Self, DomainError> {
        let queue_url = std::env::var("HEX_SQS_QUEUE_URL")
            .map_err(|_| DomainError::Internal("missing HEX_SQS_QUEUE_URL".into()))?;
        let cfg = runtime.block_on(aws_config::defaults(BehaviorVersion::latest()).load());
        Ok(Self {
            client: aws_sdk_sqs::Client::new(&cfg),
            queue_url,
            rt: runtime,
        })
    }
}

impl crate::application::ports::EventPort for SqsEventPublisher {
    fn publish(&self, topic: &str, key: &str, payload: &str) -> Result<(), DomainError> {
        // `topic` is the queue URL; the configured queue is the default.
        let queue = if topic.is_empty() { &self.queue_url } else { topic };
        let mut req = self
            .client
            .send_message()
            .queue_url(queue)
            .message_body(payload.to_string());
        if !key.is_empty() {
            let attr = MessageAttributeValue::builder()
                .data_type("String")
                .string_value(key)
                .build()
                .expect("valid message attribute");
            req = req.message_attributes("key", attr);
        }
        self.rt.block_on(req.send()).map(|_| ()).map_err(|e| {
            DomainError::Internal(format!("sqs publish {queue}: {e}"))
        })
    }
}