//! FaaS → AWS Lambda adapter (feature `lambda`).
//!
//! `services.invoke(name, payload)` runs a function synchronously
//! (`RequestResponse`) and returns its serialized response; `invokeAsync` fires
//! and forgets (`Event`). Real AWS SDK.

use std::sync::Arc;

use aws_config::BehaviorVersion;
use aws_sdk_lambda::primitives::Blob;

use crate::domain::DomainError;

pub struct LambdaAdapter {
    client: aws_sdk_lambda::Client,
    rt: Arc<tokio::runtime::Runtime>,
}

impl LambdaAdapter {
    pub fn new(rt: Arc<tokio::runtime::Runtime>) -> Self {
        let cfg = rt.block_on(aws_config::defaults(BehaviorVersion::latest()).load());
        Self {
            client: aws_sdk_lambda::Client::new(&cfg),
            rt,
        }
    }
}

impl crate::application::ports::LambdaPort for LambdaAdapter {
    fn invoke(
        &self,
        name: &str,
        payload: &serde_json::Value,
    ) -> Result<serde_json::Value, DomainError> {
        let body = serde_json::to_vec(payload)
            .map_err(|e| DomainError::Internal(format!("invoke serialize: {e}")))?;
        let resp = self
            .rt
            .block_on(
                self.client
                    .invoke()
                    .function_name(name)
                    .payload(Blob::from(body))
                    .send(),
            )
            .map_err(|e| DomainError::Internal(format!("lambda invoke {name}: {e}")))?;
        let bytes = resp
            .payload
            .map(Vec::<u8>::from)
            .unwrap_or_default();
        let json = if bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .map_err(|e| DomainError::Internal(format!("lambda {name} response: {e}")))?
        };
        Ok(json)
    }

    fn invoke_async(
        &self,
        name: &str,
        payload: &serde_json::Value,
    ) -> Result<(), DomainError> {
        let body = serde_json::to_vec(payload)
            .map_err(|e| DomainError::Internal(format!("invokeAsync serialize: {e}")))?;
        self.rt
            .block_on(
                self.client
                    .invoke()
                    .function_name(name)
                    .invocation_type(aws_sdk_lambda::types::InvocationType::Event)
                    .payload(Blob::from(body))
                    .send(),
            )
            .map(|_| ())
            .map_err(|e| DomainError::Internal(format!("lambda invokeAsync {name}: {e}")))
    }
}