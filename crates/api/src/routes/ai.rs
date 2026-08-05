use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::routing::{get, post};
use axum::Router;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use futures::stream::Stream;

use crate::state::AppState;
use appcore::models::NewAiHistory;
use storage::Repository;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/ai/status", get(ai_status))
        // 个股诊断
        .route("/api/v1/ai/diagnose/{code}", get(diagnose_stock))
        .route("/api/v1/ai/diagnose/{code}/stream", get(diagnose_stock_stream))
        // 市场研判
        .route("/api/v1/ai/market", get(market_analysis))
        .route("/api/v1/ai/market/stream", get(market_analysis_stream))
        // 趋势预测
        .route("/api/v1/ai/predict/{code}", get(predict_trend))
        .route("/api/v1/ai/predict/{code}/stream", get(predict_trend_stream))
        // 智能问答
        .route("/api/v1/ai/chat", post(chat))
        .route("/api/v1/ai/chat/stream", post(chat_stream))
}

#[derive(Serialize)]
struct AiStatus {
    configured: bool,
    model: String,
}

/// 统一错误响应
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    code: u16,
}

/// 将错误转为 (StatusCode, Json<ErrorResponse>)
fn ai_error(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    let msg = msg.into();
    tracing::error!(%msg, "AI 请求失败");
    (
        status,
        Json(ErrorResponse { error: msg, code: status.as_u16() }),
    )
}

/// GET /api/v1/ai/status - 检查 AI 配置状态
async fn ai_status(State(state): State<Arc<AppState>>) -> Json<AiStatus> {
    Json(AiStatus {
        configured: state.ai.is_configured(),
        model: std::env::var("LLM_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string()),
    })
}

#[derive(Serialize)]
struct AiResponse {
    content: String,
}

#[derive(Deserialize)]
struct ChatRequest {
    question: String,
}

/// GET /api/v1/ai/diagnose/{code} - 个股诊断（完整响应）
async fn diagnose_stock(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
) -> Result<Json<AiResponse>, (StatusCode, Json<ErrorResponse>)> {
    if !state.ai.is_configured() {
        return Err(ai_error(StatusCode::SERVICE_UNAVAILABLE, "AI 未配置，请在 .env 中设置 LLM_API_KEY"));
    }
    tracing::info!(stock = %code, "开始 AI 个股诊断");
    let content = state.ai.diagnose_stock(&code).await
        .map_err(|e| ai_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    tracing::info!(stock = %code, "AI 个股诊断完成");

    // 保存历史
    save_history(&state, "diagnose", Some(&code), None, None, &content).await;

    Ok(Json(AiResponse { content }))
}

/// GET /api/v1/ai/diagnose/{code}/stream - 个股诊断（流式）
async fn diagnose_stock_stream(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    if !state.ai.is_configured() {
        return Err(ai_error(StatusCode::SERVICE_UNAVAILABLE, "AI 未配置，请在 .env 中设置 LLM_API_KEY"));
    }
    tracing::info!(stock = %code, "开始 AI 个股诊断（流式）");
    let rx = state.ai.diagnose_stock_stream(&code).await
        .map_err(|e| ai_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let name = state.fetcher.fetch_quote(&code).await.ok().map(|q| q.name);
    let meta = NewAiHistory {
        analysis_type: "diagnose".to_string(),
        title: match &name {
            Some(n) => format!("{n}({code}) 个股诊断"),
            None => format!("{code} 个股诊断"),
        },
        stock_code: Some(code.clone()),
        stock_name: name,
        question: None,
        content: String::new(),
        model: std::env::var("LLM_MODEL").ok(),
    };

    Ok(Sse::new(forward_stream_and_save(rx, state.db.clone(), meta)))
}

/// GET /api/v1/ai/market - 市场研判（完整响应）
async fn market_analysis(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AiResponse>, (StatusCode, Json<ErrorResponse>)> {
    if !state.ai.is_configured() {
        return Err(ai_error(StatusCode::SERVICE_UNAVAILABLE, "AI 未配置，请在 .env 中设置 LLM_API_KEY"));
    }
    tracing::info!("开始 AI 市场研判");
    let content = state.ai.analyze_market().await
        .map_err(|e| ai_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    tracing::info!("AI 市场研判完成");

    save_history(&state, "market", None, None, None, &content).await;

    Ok(Json(AiResponse { content }))
}

/// GET /api/v1/ai/market/stream - 市场研判（流式）
async fn market_analysis_stream(
    State(state): State<Arc<AppState>>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    if !state.ai.is_configured() {
        return Err(ai_error(StatusCode::SERVICE_UNAVAILABLE, "AI 未配置，请在 .env 中设置 LLM_API_KEY"));
    }
    tracing::info!("开始 AI 市场研判（流式）");
    let rx = state.ai.analyze_market_stream().await
        .map_err(|e| ai_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let meta = NewAiHistory {
        analysis_type: "market".to_string(),
        title: "市场研判".to_string(),
        stock_code: None,
        stock_name: None,
        question: None,
        content: String::new(),
        model: std::env::var("LLM_MODEL").ok(),
    };

    Ok(Sse::new(forward_stream_and_save(rx, state.db.clone(), meta)))
}

/// GET /api/v1/ai/predict/{code} - 趋势预测（完整响应）
async fn predict_trend(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
) -> Result<Json<AiResponse>, (StatusCode, Json<ErrorResponse>)> {
    if !state.ai.is_configured() {
        return Err(ai_error(StatusCode::SERVICE_UNAVAILABLE, "AI 未配置，请在 .env 中设置 LLM_API_KEY"));
    }
    tracing::info!(stock = %code, "开始 AI 趋势预测");
    let content = state.ai.predict_trend(&code).await
        .map_err(|e| ai_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    tracing::info!(stock = %code, "AI 趋势预测完成");

    save_history(&state, "predict", Some(&code), None, None, &content).await;

    Ok(Json(AiResponse { content }))
}

/// GET /api/v1/ai/predict/{code}/stream - 趋势预测（流式）
async fn predict_trend_stream(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    if !state.ai.is_configured() {
        return Err(ai_error(StatusCode::SERVICE_UNAVAILABLE, "AI 未配置，请在 .env 中设置 LLM_API_KEY"));
    }
    tracing::info!(stock = %code, "开始 AI 趋势预测（流式）");
    let rx = state.ai.predict_trend_stream(&code).await
        .map_err(|e| ai_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let name = state.fetcher.fetch_quote(&code).await.ok().map(|q| q.name);
    let meta = NewAiHistory {
        analysis_type: "predict".to_string(),
        title: match &name {
            Some(n) => format!("{n}({code}) 趋势预测"),
            None => format!("{code} 趋势预测"),
        },
        stock_code: Some(code.clone()),
        stock_name: name,
        question: None,
        content: String::new(),
        model: std::env::var("LLM_MODEL").ok(),
    };

    Ok(Sse::new(forward_stream_and_save(rx, state.db.clone(), meta)))
}

/// POST /api/v1/ai/chat - 智能问答（完整响应）
async fn chat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatRequest>,
) -> Result<Json<AiResponse>, (StatusCode, Json<ErrorResponse>)> {
    if !state.ai.is_configured() {
        return Err(ai_error(StatusCode::SERVICE_UNAVAILABLE, "AI 未配置，请在 .env 中设置 LLM_API_KEY"));
    }
    tracing::info!(question = %req.question, "开始 AI 智能问答");
    let content = state.ai.chat(&req.question).await
        .map_err(|e| ai_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    tracing::info!("AI 智能问答完成");

    save_history(&state, "chat", None, None, Some(&req.question), &content).await;

    Ok(Json(AiResponse { content }))
}

/// POST /api/v1/ai/chat/stream - 智能问答（流式）
async fn chat_stream(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    if !state.ai.is_configured() {
        return Err(ai_error(StatusCode::SERVICE_UNAVAILABLE, "AI 未配置，请在 .env 中设置 LLM_API_KEY"));
    }
    tracing::info!(question = %req.question, "开始 AI 智能问答（流式）");
    let rx = state.ai.chat_stream(&req.question).await
        .map_err(|e| ai_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let question = req.question.clone();
    let meta = NewAiHistory {
        analysis_type: "chat".to_string(),
        title: if question.chars().count() > 30 {
            format!("{}…", question.chars().take(30).collect::<String>())
        } else {
            question.clone()
        },
        stock_code: None,
        stock_name: None,
        question: Some(question),
        content: String::new(),
        model: std::env::var("LLM_MODEL").ok(),
    };

    Ok(Sse::new(forward_stream_and_save(rx, state.db.clone(), meta)))
}

/// 保存一条历史记录（best-effort，失败仅记日志）
async fn save_history(
    state: &AppState,
    analysis_type: &str,
    stock_code: Option<&str>,
    stock_name: Option<&str>,
    question: Option<&str>,
    content: &str,
) {
    if content.trim().is_empty() {
        return;
    }
    let record = NewAiHistory {
        analysis_type: analysis_type.to_string(),
        title: default_title(analysis_type, stock_code, stock_name, question),
        stock_code: stock_code.map(|s| s.to_string()),
        stock_name: stock_name.map(|s| s.to_string()),
        question: question.map(|s| s.to_string()),
        content: content.to_string(),
        model: std::env::var("LLM_MODEL").ok(),
    };
    let repo = Repository::from_database(&state.db);
    if let Err(e) = repo.save_ai_history(&record).await {
        tracing::error!(error = %e, "保存 AI 历史失败");
    } else {
        tracing::info!(analysis_type = %analysis_type, "AI 历史已保存");
    }
}

/// 根据类型生成默认标题
fn default_title(
    analysis_type: &str,
    stock_code: Option<&str>,
    stock_name: Option<&str>,
    question: Option<&str>,
) -> String {
    match analysis_type {
        "diagnose" => match (stock_name, stock_code) {
            (Some(n), Some(c)) => format!("{n}({c}) 个股诊断"),
            (_, Some(c)) => format!("{c} 个股诊断"),
            _ => "个股诊断".to_string(),
        },
        "predict" => match (stock_name, stock_code) {
            (Some(n), Some(c)) => format!("{n}({c}) 趋势预测"),
            (_, Some(c)) => format!("{c} 趋势预测"),
            _ => "趋势预测".to_string(),
        },
        "market" => "市场研判".to_string(),
        "chat" => question
            .map(|q| {
                if q.chars().count() > 30 {
                    format!("{}…", q.chars().take(30).collect::<String>())
                } else {
                    q.to_string()
                }
            })
            .unwrap_or_else(|| "智能问答".to_string()),
        _ => "AI 分析".to_string(),
    }
}

/// 转发流式输出到 SSE，同时累积完整内容并在流结束后保存历史
fn forward_stream_and_save(
    mut rx: mpsc::Receiver<Result<String, anyhow::Error>>,
    db: storage::Database,
    mut meta: NewAiHistory,
) -> ReceiverStream<Result<Event, Infallible>> {
    let (sse_tx, sse_rx) = mpsc::channel::<Result<Event, Infallible>>(32);

    tokio::spawn(async move {
        let mut full = String::new();
        let mut had_error = false;

        while let Some(result) = rx.recv().await {
            match result {
                Ok(content) => {
                    full.push_str(&content);
                    if sse_tx.send(Ok(Event::default().data(content))).await.is_err() {
                        return;
                    }
                }
                Err(e) => {
                    had_error = true;
                    tracing::error!(error = %e, "AI 流式输出错误");
                    if sse_tx
                        .send(Ok(Event::default().data(format!("[错误] {}", e))))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }
        }

        // 流结束：仅在成功且内容非空时保存历史
        if !had_error && !full.trim().is_empty() {
            meta.content = full;
            let repo = Repository::from_database(&db);
            if let Err(e) = repo.save_ai_history(&meta).await {
                tracing::error!(error = %e, "保存 AI 历史失败");
            } else {
                tracing::info!(analysis_type = %meta.analysis_type, "AI 历史已保存（流式）");
            }
        }
    });

    ReceiverStream::new(sse_rx)
}
