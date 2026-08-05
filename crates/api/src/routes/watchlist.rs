use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{get, delete};
use axum::Router;
use std::sync::Arc;

use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/watchlist", get(list_watchlist).post(add_to_watchlist))
        .route("/api/v1/watchlist/{code}", delete(remove_from_watchlist))
}

/// GET /api/v1/watchlist - 获取自选股列表
async fn list_watchlist(
    State(state): State<Arc<AppState>>,
    Query(params): Query<UserParams>,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    let user_id = params.user_id.as_deref().unwrap_or("default");
    let repo = storage::Repository::from_database(&state.db);
    repo.get_watchlist(user_id)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// POST /api/v1/watchlist - 添加自选股
async fn add_to_watchlist(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AddWatchlistRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let repo = storage::Repository::from_database(&state.db);
    repo.add_to_watchlist("default", &body.code, body.group.as_deref())
        .await
        .map(|_| StatusCode::CREATED)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// DELETE /api/v1/watchlist/{code} - 移除自选股
async fn remove_from_watchlist(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let repo = storage::Repository::from_database(&state.db);
    repo.remove_from_watchlist("default", &code)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[derive(serde::Deserialize)]
struct UserParams {
    user_id: Option<String>,
}

#[derive(serde::Deserialize)]
struct AddWatchlistRequest {
    code: String,
    group: Option<String>,
}
