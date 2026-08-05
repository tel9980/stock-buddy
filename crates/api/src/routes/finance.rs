use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::Serialize;
use std::sync::Arc;

use crate::state::AppState;

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

fn err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    let msg = msg.into();
    tracing::error!(%msg, "请求失败");
    (status, Json(ErrorResponse { error: msg }))
}

/// GET /api/v1/finance/{code} - 获取个股财务指标
async fn get_financial_indicators(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
) -> Result<Json<appcore::FinancialSummary>, (StatusCode, Json<ErrorResponse>)> {
    state.fetcher
        .fetch_financial_indicators(&code)
        .await
        .map(Json)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/finance/{code}", get(get_financial_indicators))
}
