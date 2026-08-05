use axum::extract::{Query, State};
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
pub struct DateQuery {
    pub date: Option<String>,
}

/// GET /api/v1/dragontiger/list - 获取龙虎榜数据
async fn get_dragon_tiger(
    State(state): State<Arc<AppState>>,
    Query(q): Query<DateQuery>,
) -> Result<Json<Vec<appcore::DragonTigerEntry>>, (StatusCode, Json<ErrorResponse>)> {
    state.fetcher
        .fetch_dragon_tiger(q.date.as_deref())
        .await
        .map(Json)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// GET /api/v1/dragontiger/overview - 获取龙虎榜概览
async fn get_dragon_tiger_overview(
    State(state): State<Arc<AppState>>,
    Query(q): Query<DateQuery>,
) -> Result<Json<appcore::DragonTigerOverview>, (StatusCode, Json<ErrorResponse>)> {
    let entries = state.fetcher
        .fetch_dragon_tiger(q.date.as_deref())
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let net_buy_count = entries.iter().filter(|e| e.net_amount > 0.0).count() as u32;
    let net_sell_count = entries.iter().filter(|e| e.net_amount < 0.0).count() as u32;
    let institution_net_buy_total = entries.iter().map(|e| e.institution_net_buy).sum();

    Ok(Json(appcore::DragonTigerOverview {
        total_count: entries.len() as u32,
        net_buy_count,
        net_sell_count,
        institution_net_buy_total,
        entries,
        active_seats: vec![],
        timestamp: chrono::Utc::now().timestamp(),
    }))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/dragontiger/list", get(get_dragon_tiger))
        .route("/api/v1/dragontiger/overview", get(get_dragon_tiger_overview))
}
