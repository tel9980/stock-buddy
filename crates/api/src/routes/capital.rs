use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};
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

#[derive(Deserialize)]
pub struct CapitalFlowQuery {
    pub limit: Option<u32>,
}

#[derive(Deserialize)]
pub struct LimitQuery {
    pub limit: Option<u32>,
}

/// GET /api/v1/capital/{code} - 获取个股资金流向
async fn get_capital_flow(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
    Query(q): Query<CapitalFlowQuery>,
) -> Result<Json<Vec<appcore::CapitalFlow>>, (StatusCode, Json<ErrorResponse>)> {
    state.fetcher
        .fetch_capital_flow(&code, q.limit)
        .await
        .map(Json)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// GET /api/v1/capital/sectors - 获取行业板块资金流向排行
async fn get_sector_capital_flows(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<appcore::SectorCapitalFlow>>, (StatusCode, Json<ErrorResponse>)> {
    state.fetcher
        .fetch_sector_capital_flows()
        .await
        .map(Json)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// GET /api/v1/capital/north-bound - 获取北向资金流向
async fn get_north_bound_flow(
    State(state): State<Arc<AppState>>,
    Query(q): Query<LimitQuery>,
) -> Result<Json<Vec<appcore::NorthBoundFlow>>, (StatusCode, Json<ErrorResponse>)> {
    state.fetcher
        .fetch_north_bound_flow(q.limit)
        .await
        .map(Json)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// GET /api/v1/capital/north-holdings - 获取北向持股季度汇总
async fn get_north_bound_holdings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<appcore::NorthBoundHoldings>>, (StatusCode, Json<ErrorResponse>)> {
    state.fetcher
        .fetch_north_bound_holdings()
        .await
        .map(Json)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// GET /api/v1/capital/market-flow - 获取沪深两市主力资金（实时）
async fn get_market_capital_flow(
    State(state): State<Arc<AppState>>,
) -> Result<Json<appcore::MarketCapitalFlow>, (StatusCode, Json<ErrorResponse>)> {
    state.fetcher
        .fetch_market_capital_flow()
        .await
        .map(Json)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// GET /api/v1/capital/margin - 获取融资融券数据
async fn get_margin_trading(
    State(state): State<Arc<AppState>>,
    Query(q): Query<LimitQuery>,
) -> Result<Json<Vec<appcore::MarginTrading>>, (StatusCode, Json<ErrorResponse>)> {
    state.fetcher
        .fetch_margin_trading(q.limit)
        .await
        .map(Json)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// GET /api/v1/capital/overview - 获取资金流向总览
async fn get_capital_overview(
    State(state): State<Arc<AppState>>,
) -> Result<Json<appcore::CapitalFlowOverview>, (StatusCode, Json<ErrorResponse>)> {
    let (north_bound, holdings, margin, market_flow, sectors) = tokio::join!(
        state.fetcher.fetch_north_bound_flow(Some(1)),
        state.fetcher.fetch_north_bound_holdings(),
        state.fetcher.fetch_margin_trading(Some(1)),
        state.fetcher.fetch_market_capital_flow(),
        state.fetcher.fetch_sector_capital_flows(),
    );

    let sector_flows = sectors.unwrap_or_default();

    Ok(Json(appcore::CapitalFlowOverview {
        north_bound: north_bound.ok().and_then(|v| v.into_iter().next()),
        north_bound_holdings: holdings.ok().and_then(|v| v.into_iter().next()),
        margin_trading: margin.ok().and_then(|v| v.into_iter().next()),
        market_capital_flow: market_flow.ok(),
        sector_flows,
        timestamp: chrono::Utc::now().timestamp(),
    }))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/capital/{code}", get(get_capital_flow))
        .route("/api/v1/capital/sectors", get(get_sector_capital_flows))
        .route("/api/v1/capital/north-bound", get(get_north_bound_flow))
        .route("/api/v1/capital/north-holdings", get(get_north_bound_holdings))
        .route("/api/v1/capital/market-flow", get(get_market_capital_flow))
        .route("/api/v1/capital/margin", get(get_margin_trading))
        .route("/api/v1/capital/overview", get(get_capital_overview))
}
