//! MintStore → S3 object store adapter (feature `store`).
//!
//! Stores objects under keys, mirrors hex4w's S3 `StoreOutboundPort`. Real AWS
//! SDK client; configured from the standard env (region, profile, creds). The
//! adapter owns a Tokio runtime so it can be called synchronously from the Boa
//! `services.*` callbacks.

use std::sync::Arc;

use aws_config::BehaviorVersion;
use aws_sdk_s3::primitives::ByteStream;

use crate::domain::{DomainError, StoreItem};

fn s3_err(msg: impl std::fmt::Display) -> DomainError {
    DomainError::Internal(msg.to_string())
}

pub struct S3Store {
    client: aws_sdk_s3::Client,
    bucket: String,
    rt: Arc<tokio::runtime::Runtime>,
}

impl S3Store {
    /// `bucket` from env `HEX_STORE_BUCKET`; credentials/region from the
    /// standard AWS env / profile chain.
    pub fn new(bucket: impl Into<String>, rt: Arc<tokio::runtime::Runtime>) -> Self {
        let cfg = rt.block_on(aws_config::defaults(BehaviorVersion::latest()).load());
        Self {
            client: aws_sdk_s3::Client::new(&cfg),
            bucket: bucket.into(),
            rt,
        }
    }
}

impl crate::application::ports::StorePort for S3Store {
    fn save_item(&self, key: &str, content: &[u8]) -> Result<(), DomainError> {
        self.rt
            .block_on(
                self.client
                    .put_object()
                    .bucket(&self.bucket)
                    .key(key)
                    .body(ByteStream::from(content.to_vec()))
                    .send(),
            )
            .map(|_| ())
            .map_err(|e| s3_err(format!("s3 put {key}: {e}")))
    }

    fn get_item(&self, key: &str) -> Result<Option<Vec<u8>>, DomainError> {
        match self
            .rt
            .block_on(self.client.get_object().bucket(&self.bucket).key(key).send())
        {
            Ok(resp) => {
                let body = self
                    .rt
                    .block_on(resp.body.collect())
                    .map_err(|e| s3_err(format!("s3 get {key} body: {e}")))?;
                Ok(Some(body.into_bytes().to_vec()))
            }
            Err(e) if e.to_string().contains("NoSuchKey") => Ok(None),
            Err(e) => Err(s3_err(format!("s3 get {key}: {e}"))),
        }
    }

    fn list_items(&self, prefix: &str) -> Result<Vec<StoreItem>, DomainError> {
        let mut items = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut req = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(prefix);
            if let Some(t) = token.as_ref() {
                req = req.continuation_token(t);
            }
            let resp = self
                .rt
                .block_on(req.send())
                .map_err(|e| s3_err(format!("s3 list: {e}")))?;
            for o in resp.contents() {
                items.push(StoreItem {
                    key: o.key().unwrap_or("").to_string(),
                    size: o.size().unwrap_or(0),
                    last_modified: o.last_modified().map(|t| t.to_string()),
                    e_tag: o.e_tag().map(|t| t.to_string()),
                });
            }
            if resp.is_truncated().unwrap_or(false) {
                token = resp.next_continuation_token().map(|s| s.to_string());
            } else {
                break;
            }
        }
        Ok(items)
    }

    fn delete_item(&self, key: &str) -> Result<(), DomainError> {
        self.rt
            .block_on(
                self.client
                    .delete_object()
                    .bucket(&self.bucket)
                    .key(key)
                    .send(),
            )
            .map(|_| ())
            .map_err(|e| s3_err(format!("s3 delete {key}: {e}")))
    }
}