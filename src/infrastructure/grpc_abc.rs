//! XDB → gRPC `AbcService` client (features `grpc` + `abc`), via tonic.
//!
//! Mirrors hex4w `GrpcAbcAdapter`: both `sheet` (a `what=find` exec) and `exec`
//! go through `AbcService.Execute`. `where`/`sort`/`limit`/`offset` fold into
//! the `puts` JSON — the hex4w gRPC proto has no dedicated fields for them.
//!
//! Env:
//! - `HEX_XDB_GRPC_URL` (default `http://localhost:9991`)

use std::sync::Arc;

use crate::application::ports::{AbcPort, AbcRequest};
use crate::domain::{AbcResponse, DomainError};

pub mod abcp {
    tonic::include_proto!("co.onmind.grpc.proto");
}

pub struct GrpcAbcClient {
    client: abcp::abc_service_client::AbcServiceClient<tonic::transport::Channel>,
    rt: Arc<tokio::runtime::Runtime>,
}

impl GrpcAbcClient {
    pub fn new(endpoint: impl Into<String>) -> Result<Self, DomainError> {
        let endpoint = endpoint.into();
        let channel = tonic::transport::Endpoint::from_shared(endpoint.clone())
            .map_err(|e| DomainError::Internal(format!("grpc endpoint {endpoint}: {e}")))?
            .connect_lazy();
        let rt = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .map_err(|e| DomainError::Internal(format!("grpc client runtime: {e}")))?,
        );
        Ok(Self {
            client: abcp::abc_service_client::AbcServiceClient::new(channel),
            rt,
        })
    }

    fn call(&self, req: &AbcRequest) -> Result<AbcResponse, DomainError> {
        let target = abcp::AbcRequest {
            way: req.way.clone().unwrap_or_else(|| "sql".into()),
            what: req.what.clone().unwrap_or_else(|| "!".into()),
            from: req.from.clone().unwrap_or_else(|| "xykit".into()),
            some: req.some.clone().unwrap_or_default(),
            with: req.with.clone().unwrap_or_default(),
            show: req.show.clone().unwrap_or_default(),
            call: req.call.clone().unwrap_or_default(),
            puts: encode_puts(req),
        };
        let mut client = self.client.clone();
        let resp = self
            .rt
            .block_on(async { client.execute(target).await })
            .map_err(|e| DomainError::Internal(format!("grpc execute: {e}")))?;
        Ok(decode(resp.into_inner()))
    }
}

/// Fold `where`/`sort`/`limit`/`offset` into the `puts` JSON (hex4w behavior).
fn encode_puts(req: &AbcRequest) -> String {
    let mut map: serde_json::Map<String, serde_json::Value> = match &req.puts {
        Some(serde_json::Value::Object(m)) => m.clone(),
        _ => serde_json::Map::new(),
    };
    if let Some(w) = &req.where_ {
        map.insert("where".into(), w.clone());
    }
    if let Some(s) = &req.sort {
        map.insert("sort".into(), serde_json::Value::String(s.clone()));
    }
    if let Some(l) = &req.limit {
        map.insert("limit".into(), serde_json::Value::from(*l));
    }
    if let Some(o) = &req.offset {
        map.insert("offset".into(), serde_json::Value::from(*o));
    }
    if map.is_empty() {
        String::new()
    } else {
        serde_json::to_string(&map).unwrap_or_default()
    }
}

fn decode(resp: abcp::AbcResponse) -> AbcResponse {
    let status = resp.status.parse::<u16>().unwrap_or(0);
    let data = if resp.data_json.is_empty() {
        None
    } else {
        serde_json::from_str(&resp.data_json).ok()
    };
    AbcResponse {
        ok: resp.ok,
        status,
        message: resp.message,
        total: (resp.total != 0).then_some(resp.total),
        data,
    }
}

impl AbcPort for GrpcAbcClient {
    fn sheet(&self, show: &str, from: &str, some: &str) -> Result<AbcResponse, DomainError> {
        let req = AbcRequest {
            way: Some("sql".into()),
            what: Some("find".into()),
            from: Some((!from.is_empty()).then(|| from.to_string()).unwrap_or_else(|| "xykit".into())),
            some: Some((!some.is_empty()).then(|| some.to_string()).unwrap_or_else(|| "sheet".into())),
            show: Some(if show.is_empty() {
                "kit01 sheetid, kit02 name, kit03 title, kit05 model".into()
            } else {
                show.to_string()
            }),
            ..AbcRequest::default()
        };
        self.call(&req)
    }

    fn exec(&self, req: &AbcRequest) -> Result<AbcResponse, DomainError> {
        self.call(req)
    }
}