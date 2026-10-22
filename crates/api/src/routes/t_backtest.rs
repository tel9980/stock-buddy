//! 做T策略回测路由
//!
//! - `POST /api/v1/t/backtest/{code}` 按给定参数回测单只标的
//! - `POST /api/v1/t/scan/{code}`     对比多组典型参数（A~F 六个变体）

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::post;
use axum::Router;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::state::AppState;
use appcore::{KlinePeriod, TBacktestResult, TScanItem, TStrategyConfig};

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

fn err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    let msg = msg.into();
    tracing::error!(%msg, "做T回测请求失败");
    (status, Json(ErrorResponse { error: msg }))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/t/backtest/{code}", post(run_backtest))
        .route("/api/v1/t/scan/{code}", post(scan_params))
}

#[derive(Deserialize)]
struct BacktestReq {
    /// K 线周期，默认 daily
    period: Option<String>,
    /// K 线根数，默认 500
    limit: Option<u32>,
    /// 策略参数；缺省则使用默认配置
    config: Option<TStrategyConfig>,
}

#[derive(Deserialize)]
struct ScanReq {
    period: Option<String>,
    limit: Option<u32>,
}

/// POST /api/v1/t/backtest/{code}
async fn run_backtest(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
    Json(req): Json<BacktestReq>,
) -> Result<Json<TBacktestResult>, (StatusCode, Json<ErrorResponse>)> {
    let period: KlinePeriod = req
        .period
        .as_deref()
        .unwrap_or("daily")
        .parse()
        .unwrap_or(KlinePeriod::Daily);
    let limit = req.limit.unwrap_or(500);
    let config = req.config.unwrap_or_default();

    let klines = state
        .fetcher
        .fetch_kline(&code, &period, None, None, Some(limit))
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("获取K线失败: {}", e)))?;

    if klines.len() < 80 {
        return Err(err(
            StatusCode::BAD_REQUEST,
            format!(
                "K线数据不足（仅 {} 根），做T回测至少需要 80 根。请增大 limit 或改用日线周期。",
                klines.len()
            ),
        ));
    }

    let result = analyzer::run_t_backtest(&klines, &code, &format!("{:?}", period), &config);
    Ok(Json(result))
}

/// POST /api/v1/t/scan/{code}
///
/// 对同一标的跑多组典型参数，便于横向对比哪种出场/过滤方式更优。
async fn scan_params(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
    Json(req): Json<ScanReq>,
) -> Result<Json<Vec<TScanItem>>, (StatusCode, Json<ErrorResponse>)> {
    let period: KlinePeriod = req
        .period
        .as_deref()
        .unwrap_or("daily")
        .parse()
        .unwrap_or(KlinePeriod::Daily);
    let limit = req.limit.unwrap_or(500);

    let klines = state
        .fetcher
        .fetch_kline(&code, &period, None, None, Some(limit))
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("获取K线失败: {}", e)))?;

    if klines.len() < 80 {
        return Err(err(
            StatusCode::BAD_REQUEST,
            format!("K线数据不足（仅 {} 根），至少需要 80 根。", klines.len()),
        ));
    }

    Ok(Json(analyzer::scan_t_params(&klines)))
}
