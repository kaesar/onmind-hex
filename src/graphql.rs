//! GraphQL BFF (feature `graphql`).
//!
//! Synchronous GraphQL layer over the scripting use case and the XDB sheet
//! read. Executed via `Schema::execute_sync` inside Feather's HTTP handler —
//! resolvers stay synchronous, no extra async runtime.

use async_graphql::Context;
use async_graphql::{
    EmptyMutation, EmptySubscription, Json, Object, Result, Schema, SimpleObject,
};
use std::sync::Arc;

use crate::domain::{AbcResponse, ScriptResult};
use crate::graph::Graph;

/// Per-request state provided to the resolvers.
pub struct AppState {
    pub graph: Arc<Graph>,
}

#[derive(SimpleObject)]
pub struct HealthView {
    status: &'static str,
    service: &'static str,
    version: &'static str,
}

#[derive(SimpleObject)]
pub struct ScriptView {
    pub value: Json<serde_json::Value>,
    pub stdout: String,
    pub stderr: Option<String>,
}

#[derive(SimpleObject)]
pub struct SheetView {
    pub ok: bool,
    pub status: u16,
    pub message: String,
    pub total: Option<i64>,
    pub data: Option<Json<serde_json::Value>>,
}

#[derive(Default)]
pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn health(&self) -> HealthView {
        HealthView {
            status: "healthy",
            service: "hex",
            version: "0.1.0",
        }
    }

    async fn execute(&self, ctx: &Context<'_>, script: String) -> Result<ScriptView> {
        let state = ctx.data::<AppState>()?;
        let result: ScriptResult = state.graph.scripting.execute(&script)?;
        Ok(ScriptView {
            value: Json(result.value.unwrap_or(serde_json::Value::Null)),
            stdout: result.stdout,
            stderr: result.stderr,
        })
    }

    async fn sheet(
        &self,
        ctx: &Context<'_>,
        show: String,
        from: Option<String>,
        some: Option<String>,
    ) -> Result<SheetView> {
        let state = ctx.data::<AppState>()?;
        let from = from.unwrap_or_else(|| "xykit".to_string());
        let some = some.unwrap_or_else(|| "sheet".to_string());
        let resp: AbcResponse = state.graph.abc_sheet(&show, &from, &some)?;
        Ok(SheetView {
            ok: resp.ok,
            status: resp.status,
            message: resp.message,
            total: resp.total.map(|t| t as i64),
            data: resp.data.map(Json),
        })
    }
}

pub type AppSchema = Schema<QueryRoot, EmptyMutation, EmptySubscription>;

pub fn build_schema() -> AppSchema {
    Schema::build(
        QueryRoot,
        EmptyMutation,
        EmptySubscription,
    )
    .finish()
}