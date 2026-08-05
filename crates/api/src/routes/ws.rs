use axum::extract::{Query, State, WebSocketUpgrade};
use axum::extract::ws::{WebSocket, Message};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;

use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/ws/quotes", get(ws_quotes))
}

/// WS /ws/quotes?codes=000001,600000 - 实时行情推送
async fn ws_quotes(
    ws: WebSocketUpgrade,
    Query(params): Query<WsParams>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let codes: Vec<String> = params
        .codes
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    ws.on_upgrade(move |socket| handle_ws_quotes(socket, codes, state))
}

async fn handle_ws_quotes(mut socket: WebSocket, codes: Vec<String>, state: Arc<AppState>) {
    let mut ticker = interval(Duration::from_secs(5));

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let code_refs: Vec<&str> = codes.iter().map(|s| s.as_str()).collect();
                match state.fetcher.fetch_quotes(&code_refs).await {
                    Ok(quotes) => {
                        let json = serde_json::to_string(&quotes).unwrap_or_default();
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::warn!("WS fetch quotes error: {}", e);
                    }
                }
            }
            msg = socket.recv() => {
                if msg.is_none() {
                    break;
                }
            }
        }
    }
}

#[derive(serde::Deserialize)]
struct WsParams {
    codes: String,
}
