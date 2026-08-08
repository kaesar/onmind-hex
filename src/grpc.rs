//! gRPC server (feature `grpc`).
//!
//! Serves the XDB `/abc` contract over HTTP/2 (tonic + prost, codegen from
//! `proto/xdb.proto`). It runs on its own Tokio runtime in a background thread,
//! coexisting with Feather's synchronous HTTP server.

use std::net::SocketAddr;
use std::sync::Arc;

use tonic::transport::Server;
use tonic::{Request, Response, Status};

use crate::domain::AbcResponse;
use crate::graph::Graph;

pub mod pb {
    tonic::include_proto!("hex4w.xdb");
}

use pb::xdb_server::{Xdb, XdbServer};

pub struct XdbService {
    graph: Arc<Graph>,
}

#[tonic::async_trait]
impl Xdb for XdbService {
    async fn sheet(
        &self,
        request: Request<pb::SheetRequest>,
    ) -> Result<Response<pb::SheetResponse>, Status> {
        let r = request.into_inner();
        let abcd: AbcResponse = self
            .graph
            .abc_sheet(&r.show, &r.from, &r.some)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(pb::SheetResponse {
            ok: abcd.ok,
            status: abcd.status as u32,
            message: abcd.message,
            total: abcd.total.unwrap_or(0),
            data: abcd.data.map(|d| d.to_string()).unwrap_or_default(),
        }))
    }
}

/// Build the gRPC service for a pre-wired `Graph`.
pub fn serve(graph: &Arc<Graph>) {
    let port: u16 = std::env::var("HEX_GRPC_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(50051);
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().expect("grpc addr");

    let xdb = XdbService { graph: Arc::clone(graph) };

    // Run the tonic server on its own Tokio runtime in a background thread.
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("grpc tokio runtime");
        rt.block_on(async move {
            Server::builder()
                .add_service(XdbServer::new(xdb))
                .serve(addr)
                .await
                .expect("gRPC server failed");
        });
    });

    println!("- gRPC : {addr} (hex4w.xdb.Xdb/Sheet)");
}