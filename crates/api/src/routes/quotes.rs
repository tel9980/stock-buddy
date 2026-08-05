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

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/quotes/{code}", get(get_quote))
        .route("/api/v1/quotes", get(batch_quotes))
}

/// GET /api/v1/quotes/{code} - 获取个股实时行情
async fn get_quote(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
) -> Result<Json<appcore::Quote>, (StatusCode, Json<ErrorResponse>)> {
    state
        .fetcher
        .fetch_quote(&code)
        .await
        .map(Json)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// GET /api/v1/quotes?codes=000001,000002 - 批量获取行情
async fn batch_quotes(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<BatchParams>,
) -> Result<Json<Vec<appcore::Quote>>, (StatusCode, Json<ErrorResponse>)> {
    let codes: Vec<&str> = params.codes.split(',').collect();
    state
        .fetcher
        .fetch_quotes(&codes)
        .await
        .map(Json)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[derive(serde::Deserialize)]
struct BatchParams {
    codes: String,
}
