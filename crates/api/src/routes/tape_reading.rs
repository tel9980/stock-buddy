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
    Router::new().route(
        "/api/v1/analysis/tape-reading/{code}",
        get(tape_reading_analysis),
    )
}

/// GET /api/v1/analysis/tape-reading/{code}?period=daily&limit=120&dt_date=YYYYMMDD
///
/// 盘口解读：复用量价分析结论，再用资金流向（主力/超大单/大单/小单）交叉验证，
/// 并用龙虎榜席位确认机构参与度。
async fn tape_reading_analysis(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
    Query(params): Query<TrParams>,
) -> Result<Json<analyzer::TapeReadingAnalysis>, (StatusCode, Json<ErrorResponse>)> {
    let period: KlinePeriod = params
        .period
        .as_deref()
        .unwrap_or("daily")
        .parse()
        .unwrap_or(KlinePeriod::Daily);
    let limit = params.limit.unwrap_or(120);

    // 并行获取：K线、个股资金流向、龙虎榜列表
    let (klines, capital_flow, dt_list) = tokio::join!(
        state
            .fetcher
            .fetch_kline(&code, &period, None, None, Some(limit)),
        state.fetcher.fetch_capital_flow(&code, Some(15)),
        state.fetcher.fetch_dragon_tiger(params.dt_date.as_deref()),
    );

    let klines = klines.map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if klines.is_empty() {
        return Err(err(StatusCode::NOT_FOUND, "无可用K线数据"));
    }

    let capital_flow = capital_flow.unwrap_or_default();
    // 龙虎榜按代码匹配（取最近一条）
    let dragon_tiger = dt_list
        .ok()
        .and_then(|list| list.into_iter().find(|e| e.code == code));

    let vp = analyzer::analyze_volume_price(&klines);
    let analysis = analyzer::analyze_tape_reading(&code, &vp, &capital_flow, dragon_tiger.as_ref());
    Ok(Json(analysis))
}

#[derive(serde::Deserialize)]
struct TrParams {
    period: Option<String>,
    limit: Option<u32>,
    dt_date: Option<String>,
}
