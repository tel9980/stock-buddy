use axum::extract::{Path, Query, State};
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
        .route("/api/v1/market/overview", get(market_overview))
        .route("/api/v1/market/sectors", get(sector_ranks))
        .route("/api/v1/market/sectors/{code}/stocks", get(sector_stocks))
}

/// GET /api/v1/market/overview - 大盘总览
async fn market_overview(
    State(state): State<Arc<AppState>>,
) -> Result<Json<appcore::MarketOverview>, (StatusCode, Json<ErrorResponse>)> {
    state
        .fetcher
        .fetch_market_overview()
        .await
        .map(Json)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// GET /api/v1/market/sectors?type=industry&sort=change_pct
async fn sector_ranks(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SectorParams>,
) -> Result<Json<Vec<appcore::SectorRank>>, (StatusCode, Json<ErrorResponse>)> {
    let sector_type = params.r#type.as_deref().unwrap_or("industry");
    state
        .fetcher
        .fetch_sector_ranks(sector_type)
        .await
        .map(Json)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// GET /api/v1/market/sectors/{code}/stocks?page=1&size=50
async fn sector_stocks(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
    Query(params): Query<SectorStockParams>,
) -> Result<Json<SectorStocksResponse>, (StatusCode, Json<ErrorResponse>)> {
    let page = params.page.unwrap_or(1);
    let size = params.size.unwrap_or(50);
    let (stocks, total) = state
        .fetcher
        .fetch_sector_stocks(&code, page, size)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(SectorStocksResponse { stocks, total, page, size }))
}

#[derive(serde::Deserialize)]
struct SectorParams {
    #[serde(rename = "type")]
    r#type: Option<String>,
}

#[derive(serde::Deserialize)]
struct SectorStockParams {
    page: Option<u32>,
    size: Option<u32>,
}

#[derive(serde::Serialize)]
struct SectorStocksResponse {
    stocks: Vec<appcore::SectorStock>,
    total: u32,
    page: u32,
    size: u32,
}
