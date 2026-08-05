use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use storage::Repository;

use crate::state::AppState;

/// 统一错误响应
#[derive(serde::Serialize)]
struct ErrorResponse {
    error: String,
    code: u16,
}

fn err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (status, Json(ErrorResponse { error: msg.into(), code: status.as_u16() }))
}

#[derive(Debug, Deserialize)]
struct HistoryQuery {
    /// 按分析类型过滤：diagnose | predict | market | chat
    #[serde(rename = "type")]
    analysis_type: Option<String>,
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default)]
    offset: u32,
}

fn default_limit() -> u32 {
    50
}

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/api/v1/ai/history", axum::routing::get(list_history))
        .route(
            "/api/v1/ai/history/{id}",
            axum::routing::get(get_history).delete(delete_history),
        )
}

/// GET /api/v1/ai/history - 列出 AI 分析历史
async fn list_history(
    State(state): State<Arc<AppState>>,
    Query(q): Query<HistoryQuery>,
) -> Json<Vec<appcore::models::AiHistoryItem>> {
    let repo = Repository::from_database(&state.db);
    let items = repo
        .list_ai_history(q.analysis_type.as_deref(), q.limit, q.offset)
        .await
        .unwrap_or_default();
    Json(items)
}

/// GET /api/v1/ai/history/{id} - 获取单条历史详情
async fn get_history(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<appcore::models::AiAnalysisHistory>, (StatusCode, Json<ErrorResponse>)> {
    let repo = Repository::from_database(&state.db);
    match repo.get_ai_history(id).await {
        Ok(Some(h)) => Ok(Json(h)),
        Ok(None) => Err(err(StatusCode::NOT_FOUND, "未找到该历史记录")),
        Err(e) => Err(err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// DELETE /api/v1/ai/history/{id} - 删除一条历史
async fn delete_history(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let repo = Repository::from_database(&state.db);
    match repo.delete_ai_history(id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => Err(err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}
