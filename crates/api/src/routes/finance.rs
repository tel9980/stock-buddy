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
///
/// 上游没有数据时返回 **404 而不是 200 + 空对象**：空对象会让前端渲染出一张
/// 空白表格，用户既看不出是「查不到」也看不出是「接口坏了」。
async fn get_financial_indicators(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
) -> Result<Json<appcore::FinancialSummary>, (StatusCode, Json<ErrorResponse>)> {
    state.fetcher
        .fetch_financial_indicators(&code)
        .await
        .map(Json)
        .map_err(|e| match e {
            appcore::AppError::NotFound(_) => err(StatusCode::NOT_FOUND, e.to_string()),
            _ => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        })
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/finance/{code}", get(get_financial_indicators))
}
