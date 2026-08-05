use std::sync::Arc;
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::routes;
use crate::state::AppState;

pub struct App {
    pub router: Router,
    pub state: Arc<AppState>,
}

pub async fn create_app() -> anyhow::Result<App> {
    let listen_addr = std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "data/stock.db".to_string());

    let db = storage::Database::new(&db_path).await?;
    let fetcher = Arc::new(fetcher::EastmoneyFetcher::new());
    let ai_service = analyzer::AiService::new(
        analyzer::ai::LlmClient::from_env(),
        fetcher.clone(),
    );

    let state = Arc::new(AppState::new(listen_addr, fetcher, db, ai_service));

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let router = Router::new()
        .merge(routes::quotes::router())
        .merge(routes::kline::router())
        .merge(routes::market::router())
        .merge(routes::watchlist::router())
        .merge(routes::ai::router())
        .merge(routes::analysis::router())
        .merge(routes::capital::router())
        .merge(routes::limitup::router())
        .merge(routes::dragontiger::router())
        .merge(routes::finance::router())
        .merge(routes::ai_history::router())
        .merge(routes::ws::router())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    Ok(App {
        router,
        state,
    })
}
