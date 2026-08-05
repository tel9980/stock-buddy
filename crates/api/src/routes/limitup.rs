use axum::extract::State;
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

/// GET /api/v1/limitup/review - 获取涨停复盘全景
async fn get_limit_up_review(
    State(state): State<Arc<AppState>>,
) -> Result<Json<appcore::LimitUpReview>, (StatusCode, Json<ErrorResponse>)> {
    state.fetcher
        .fetch_limit_up_review()
        .await
        .map(Json)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// GET /api/v1/limitup/ups - 获取涨停股列表
async fn get_limit_ups(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<appcore::LimitUpStock>>, (StatusCode, Json<ErrorResponse>)> {
    state.fetcher
        .fetch_limit_ups()
        .await
        .map(Json)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// GET /api/v1/limitup/downs - 获取跌停股列表
async fn get_limit_downs(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<appcore::LimitDownStock>>, (StatusCode, Json<ErrorResponse>)> {
    state.fetcher
        .fetch_limit_downs()
        .await
        .map(Json)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// GET /api/v1/limitup/broken - 获取炸板股列表
async fn get_broken_boards(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<appcore::BrokenBoardStock>>, (StatusCode, Json<ErrorResponse>)> {
    state.fetcher
        .fetch_broken_boards()
        .await
        .map(Json)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/limitup/review", get(get_limit_up_review))
        .route("/api/v1/limitup/ups", get(get_limit_ups))
        .route("/api/v1/limitup/downs", get(get_limit_downs))
        .route("/api/v1/limitup/broken", get(get_broken_boards))
}
