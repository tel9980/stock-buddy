use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::Serialize;
use std::sync::Arc;

use crate::state::AppState;
use appcore::KlinePeriod;

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
        .route("/api/v1/analysis/volume-price/{code}", get(volume_price_analysis))
}

/// GET /api/v1/analysis/volume-price/{code}?period=daily&limit=120
async fn volume_price_analysis(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
    Query(params): Query<VpParams>,
) -> Result<Json<analyzer::VolumePriceAnalysis>, (StatusCode, Json<ErrorResponse>)> {
    let period: KlinePeriod = params
        .period
        .as_deref()
        .unwrap_or("daily")
        .parse()
        .unwrap_or(KlinePeriod::Daily);
    let limit = params.limit.unwrap_or(120);

    let klines = state
        .fetcher
        .fetch_kline(&code, &period, None, None, Some(limit))
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if klines.is_empty() {
        return Err(err(StatusCode::NOT_FOUND, "无可用K线数据"));
    }

    let analysis = analyzer::analyze_volume_price(&klines);
    Ok(Json(analysis))
}

#[derive(serde::Deserialize)]
struct VpParams {
    period: Option<String>,
    limit: Option<u32>,
}
