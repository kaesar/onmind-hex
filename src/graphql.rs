//! GraphQL gateway over the XDB `/abc` protocol (feature `graphql`).
//!
//! Mirrors hex4w `AbcGraphqlResolver`/`AbcSheetTrait`: the only queries are
//! `abcSheet(show, from, some)` and `abcSheets(requests)` against the XDB
//! `/abc` endpoint (via `AbcPort.sheet`). Feather is fully synchronous, so the
//! resolvers run with `block_on` — no extra async runtime. `defaults` for the
//! XDB payload (show columns / from / some) live in the `AbcPort` adapters,
//! matching hex4w `AbcWebClient.sheet`.

use async_graphql::{Context, EmptyMutation, EmptySubscription, InputObject, Json, Object, Result, Schema, SimpleObject};
use std::sync::Arc;

use crate::domain::AbcResponse;
use crate::graph::Graph;

/// Per-request state provided to the resolvers.
pub struct AppState {
    pub graph: Arc<Graph>,
}

/// XDB `/abc` `find` response (hex4w `SheetResponseDto`).
#[derive(SimpleObject)]
pub struct SheetResponse {
    pub ok: bool,
    pub status: u16,
    pub message: String,
    pub total: Option<i64>,
    pub data: Option<Json<serde_json::Value>>,
}

/// One sheet query input (hex4w `AbcSheetInput`).
#[derive(InputObject)]
pub struct AbcSheetInput {
    pub show: String,
    pub from: String,
    pub some: String,
}

impl AbcSheetInput {
    fn to_args(&self) -> (&str, &str, &str) {
        (&self.show, &self.from, &self.some)
    }
}

#[derive(Default)]
pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// One XDB `/abc` sheet read (hex4w `abcSheet`).
    async fn abc_sheet(
        &self,
        ctx: &Context<'_>,
        show: String,
        from: String,
        some: String,
    ) -> Result<SheetResponse> {
        let state = ctx.data::<AppState>()?;
        let resp: AbcResponse = state.graph.abc_sheet(&show, &from, &some)?;
        Ok(map_sheet(resp))
    }

    /// Batch XDB `/abc` sheet reads, one call per request (hex4w `abcSheets`).
    async fn abc_sheets(
        &self,
        ctx: &Context<'_>,
        requests: Vec<AbcSheetInput>,
    ) -> Result<Vec<SheetResponse>> {
        let state = ctx.data::<AppState>()?;
        let mut out = Vec::with_capacity(requests.len());
        for r in requests {
            let (show, from, some) = r.to_args();
            let resp: AbcResponse = state.graph.abc_sheet(show, from, some)?;
            out.push(map_sheet(resp));
        }
        Ok(out)
    }
}

fn map_sheet(r: AbcResponse) -> SheetResponse {
    SheetResponse {
        ok: r.ok,
        status: r.status,
        message: r.message,
        total: r.total.map(|t| t as i64),
        data: r.data.map(Json),
    }
}

pub type AppSchema = Schema<QueryRoot, EmptyMutation, EmptySubscription>;

pub fn build_schema() -> AppSchema {
    Schema::build(QueryRoot, EmptyMutation, EmptySubscription).finish()
}