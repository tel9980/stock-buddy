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
    Router::new().route("/api/v1/kline/{code}", get(get_kline))
}

/// GET /api/v1/kline/{code}?period=daily&start=2026-01-01&end=2026-08-01&limit=365
async fn get_kline(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
    Query(params): Query<KlineParams>,
) -> Result<Json<Vec<appcore::Kline>>, (StatusCode, Json<ErrorResponse>)> {
    let period = match params.period.as_deref().unwrap_or("daily") {
        "weekly" => KlinePeriod::Weekly,
        "monthly" => KlinePeriod::Monthly,
        "1" | "minute1" => KlinePeriod::Minute1,
        "5" | "minute5" => KlinePeriod::Minute5,
        "15" | "minute15" => KlinePeriod::Minute15,
        "30" | "minute30" => KlinePeriod::Minute30,
        "60" | "minute60" => KlinePeriod::Minute60,
        _ => KlinePeriod::Daily,
    };

    state
        .fetcher
        .fetch_kline(
            &code,
            &period,
            params.start.as_deref(),
            params.end.as_deref(),
            params.limit,
        )
        .await
        .map(Json)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[derive(serde::Deserialize)]
struct KlineParams {
    period: Option<String>,
    start: Option<String>,
    end: Option<String>,
    limit: Option<u32>,
}
