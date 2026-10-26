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
        // AI 配置（页面可编辑）
        .route("/api/v1/ai/config", get(get_ai_config).put(put_ai_config))
        // 拉取上游可用模型列表（设置页「拉取模型」按钮）
        .route("/api/v1/ai/models", get(list_models))
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
    /// 是否至少有一个可用的 AI 引擎（LLM 或妙想）
    configured: bool,
    /// 实际会使用的引擎：`llm` / `mx` / `none`
    engine: String,
    model: String,
    /// 妙想是否已配置（前端据此显示可选项）
    mx_configured: bool,
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
    let llm_ok = state.ai.is_configured();
    let mx_ok = state.mx().is_configured();
    let engine = if llm_ok {
        "llm"
    } else if mx_ok {
        "mx"
    } else {
        "none"
    };
    Json(AiStatus {
        configured: llm_ok || mx_ok,
        engine: engine.to_string(),
        model: if llm_ok {
            state.ai_config.get().model
        } else {
            "妙想金融大模型".to_string()
        },
        mx_configured: mx_ok,
    })
}

/// GET /api/v1/ai/config - 读取当前 AI 配置（API Key 打码）
async fn get_ai_config(State(state): State<Arc<AppState>>) -> Json<crate::ai_config::AiSettings> {
    Json(state.ai_config.masked())
}

/// PUT /api/v1/ai/config - 保存 AI 配置，立即生效并落盘
///
/// 传 `api_key` 为掩码值（含 `****`）表示「不修改 key，仅改 url/model」。
async fn put_ai_config(
    State(state): State<Arc<AppState>>,
    Json(mut body): Json<crate::ai_config::AiSettings>,
) -> Result<Json<crate::ai_config::AiSettings>, (StatusCode, Json<ErrorResponse>)> {
    // 打码回传时保留原 key，避免前端只改模型却把 key 清空
    if body.api_key.contains("****") {
        body.api_key = state.ai_config.get().api_key;
    }
    state
        .ai_config
        .save(body.clone())
        .map_err(|e| ai_error(StatusCode::BAD_REQUEST, e))?;

    // 热替换 LLM 客户端，后续请求立刻用新配置
    state.ai.set_llm(analyzer::ai::LlmClient::new(analyzer::ai::LlmConfig {
        api_key: body.api_key,
        base_url: body.base_url,
        model: body.model,
        ..Default::default()
    }));

    tracing::info!("AI 配置已更新（页面保存），已热生效");
    Ok(Json(state.ai_config.masked()))
}

#[derive(Serialize)]
struct ModelListResponse {
    /// 上游返回的模型 id 列表（已去重、升序）
    models: Vec<String>,
}

/// GET /api/v1/ai/models - 代理拉取上游 `/models`，给设置页做下拉选择
///
/// 由后端代理而不是浏览器直连，原因有三：
/// 1. 上游多半没配 CORS，浏览器直连会被拦；
/// 2. Base URL 可能是内网地址，浏览器所在机器未必能通，容器内却能通；
/// 3. API Key 不用下发到前端，避免在浏览器里被读到。
///
/// 支持 OpenAI 兼容的两种返回形态：`{"data":[{"id":...}]}` 与 `{"models":[...]}`；
/// 部分网关用裸数组。取不到时返回 502 并说明原因，让用户知道该查什么。
async fn list_models(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ModelListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let cfg = state.ai_config.get();
    if cfg.api_key.trim().is_empty() {
        return Err(ai_error(
            StatusCode::BAD_REQUEST,
            "尚未配置 API Key，请先填写后再拉取模型列表",
        ));
    }

    let url = format!("{}/models", cfg.base_url.trim_end_matches('/'));
    // 模型列表接口要快，别用 LLM 那种 120s 超时
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| ai_error(StatusCode::INTERNAL_SERVER_ERROR, format!("HTTP 客户端创建失败: {e}")))?;

    let resp = client
        .get(&url)
        .bearer_auth(cfg.api_key.trim())
        .send()
        .await
        .map_err(|e| {
            ai_error(
                StatusCode::BAD_GATEWAY,
                format!("无法连接 {url}：{e}（若为内网地址，请确认容器能访问该网段）"),
            )
        })?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        // 上游原文可能很长（含 HTML 错误页），截断后再回传
        let snippet: String = body.chars().take(300).collect();
        return Err(ai_error(
            StatusCode::BAD_GATEWAY,
            format!("上游返回 {status}：{snippet}"),
        ));
    }

    let models = parse_model_ids(&body).map_err(|e| {
        ai_error(
            StatusCode::BAD_GATEWAY,
            format!("{e}；该服务可能不提供 /models 接口（例如同花顺金融数据 API 就没有）"),
        )
    })?;

    if models.is_empty() {
        return Err(ai_error(
            StatusCode::BAD_GATEWAY,
            "上游 /models 返回了但没有可用模型 id",
        ));
    }

    tracing::info!(count = models.len(), "已拉取上游模型列表");
    Ok(Json(ModelListResponse { models }))
}

/// 解析 `/models` 响应，兼容三种常见形态。
fn parse_model_ids(body: &str) -> Result<Vec<String>, String> {
    let v: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| format!("上游响应不是合法 JSON: {e}"))?;

    let pick = |item: &serde_json::Value| -> Option<String> {
        item.get("id")
            .and_then(|x| x.as_str())
            .or_else(|| item.get("name").and_then(|x| x.as_str()))
            .or_else(|| item.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };

    let arr = if let Some(a) = v.get("data").and_then(|d| d.as_array()) {
        a.clone()
    } else if let Some(a) = v.get("models").and_then(|m| m.as_array()) {
        a.clone()
    } else if let Some(a) = v.as_array() {
        a.clone()
    } else {
        return Err("响应里没有 data/models 数组".to_string());
    };

    let mut ids: Vec<String> = arr.iter().filter_map(pick).collect();
    ids.sort();
    ids.dedup();
    Ok(ids)
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
    ensure_engine(&state)?;
    tracing::info!(stock = %code, "开始 AI 个股诊断");
    let content = if state.ai.is_configured() {
        state.ai.diagnose_stock(&code).await
            .map_err(|e| ai_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        mx_diagnose(&state, &code).await?
    };
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
    ensure_engine(&state)?;
    tracing::info!(stock = %code, "开始 AI 个股诊断（流式）");
    let rx = if state.ai.is_configured() {
        state.ai.diagnose_stock_stream(&code).await
            .map_err(|e| ai_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        match mx_diagnose(&state, &code).await {
            Ok(t) => mx_mock_stream(t),
            Err((_, Json(e))) => mx_stream_fallback(e.error),
        }
    };

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
        model: engine_model(&state),
    };

    Ok(Sse::new(forward_stream_and_save(rx, state.db.clone(), meta)))
}

/// GET /api/v1/ai/market - 市场研判（完整响应）
async fn market_analysis(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AiResponse>, (StatusCode, Json<ErrorResponse>)> {
    ensure_engine(&state)?;
    tracing::info!("开始 AI 市场研判");
    let content = if state.ai.is_configured() {
        state.ai.analyze_market().await
            .map_err(|e| ai_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        mx_market(&state).await?
    };
    tracing::info!("AI 市场研判完成");

    save_history(&state, "market", None, None, None, &content).await;

    Ok(Json(AiResponse { content }))
}

/// GET /api/v1/ai/market/stream - 市场研判（流式）
async fn market_analysis_stream(
    State(state): State<Arc<AppState>>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    ensure_engine(&state)?;
    tracing::info!("开始 AI 市场研判（流式）");
    let rx = if state.ai.is_configured() {
        state.ai.analyze_market_stream().await
            .map_err(|e| ai_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        match mx_market(&state).await {
            Ok(t) => mx_mock_stream(t),
            Err((_, Json(e))) => mx_stream_fallback(e.error),
        }
    };

    let meta = NewAiHistory {
        analysis_type: "market".to_string(),
        title: "市场研判".to_string(),
        stock_code: None,
        stock_name: None,
        question: None,
        content: String::new(),
        model: engine_model(&state),
    };

    Ok(Sse::new(forward_stream_and_save(rx, state.db.clone(), meta)))
}

/// GET /api/v1/ai/predict/{code} - 趋势预测（完整响应）
async fn predict_trend(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
) -> Result<Json<AiResponse>, (StatusCode, Json<ErrorResponse>)> {
    ensure_engine(&state)?;
    tracing::info!(stock = %code, "开始 AI 趋势预测");
    let content = if state.ai.is_configured() {
        state.ai.predict_trend(&code).await
            .map_err(|e| ai_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        mx_predict(&state, &code).await?
    };
    tracing::info!(stock = %code, "AI 趋势预测完成");

    save_history(&state, "predict", Some(&code), None, None, &content).await;

    Ok(Json(AiResponse { content }))
}

/// GET /api/v1/ai/predict/{code}/stream - 趋势预测（流式）
async fn predict_trend_stream(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    ensure_engine(&state)?;
    tracing::info!(stock = %code, "开始 AI 趋势预测（流式）");
    let rx = if state.ai.is_configured() {
        state.ai.predict_trend_stream(&code).await
            .map_err(|e| ai_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        match mx_predict(&state, &code).await {
            Ok(t) => mx_mock_stream(t),
            Err((_, Json(e))) => mx_stream_fallback(e.error),
        }
    };

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
        model: engine_model(&state),
    };

    Ok(Sse::new(forward_stream_and_save(rx, state.db.clone(), meta)))
}

/// POST /api/v1/ai/chat - 智能问答（完整响应）
async fn chat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatRequest>,
) -> Result<Json<AiResponse>, (StatusCode, Json<ErrorResponse>)> {
    ensure_engine(&state)?;
    tracing::info!(question = %req.question, "开始 AI 智能问答");
    let content = if state.ai.is_configured() {
        state.ai.chat(&req.question).await
            .map_err(|e| ai_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        mx_ask(&state, &req.question).await?
    };
    tracing::info!("AI 智能问答完成");

    save_history(&state, "chat", None, None, Some(&req.question), &content).await;

    Ok(Json(AiResponse { content }))
}

/// POST /api/v1/ai/chat/stream - 智能问答（流式）
async fn chat_stream(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    ensure_engine(&state)?;
    tracing::info!(question = %req.question, "开始 AI 智能问答（流式）");
    let rx = if state.ai.is_configured() {
        state.ai.chat_stream(&req.question).await
            .map_err(|e| ai_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        match mx_ask(&state, &req.question).await {
            Ok(t) => mx_mock_stream(t),
            Err((_, Json(e))) => mx_stream_fallback(e.error),
        }
    };

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
        model: engine_model(&state),
    };

    Ok(Sse::new(forward_stream_and_save(rx, state.db.clone(), meta)))
}


/// 引擎选择：LLM 已配置就用 LLM，否则回落妙想。两者都没配才报错。
fn ensure_engine(state: &AppState) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if state.ai.is_configured() || state.mx().is_configured() {
        return Ok(());
    }
    Err(ai_error(
        StatusCode::SERVICE_UNAVAILABLE,
        "AI 未配置：请到「设置」页填写大模型 Key 或东方财富妙想 Key",
    ))
}

/// 妙想没有流式 API，只能拿到完整答案。为了不让人对着空白页干等，
/// 这里按句子切成小段做**伪流式**推送（每段之间 25ms）。
fn mx_mock_stream(text: String) -> mpsc::Receiver<Result<String, anyhow::Error>> {
    let (tx, rx) = mpsc::channel(32);
    tokio::spawn(async move {
        let mut buf = String::new();
        for ch in text.chars() {
            buf.push(ch);
            // 在句末或段落处切块，避免把一个词拆开
            if matches!(ch, '。' | '！' | '？' | '；' | '\n') || buf.chars().count() >= 40 {
                if tx.send(Ok(std::mem::take(&mut buf))).await.is_err() {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        }
        if !buf.is_empty() {
            let _ = tx.send(Ok(buf)).await;
        }
    });
    rx
}

/// 妙想故障统一转成 SSE 内的错误事件，避免前端只看到空流。
fn mx_stream_fallback(msg: String) -> mpsc::Receiver<Result<String, anyhow::Error>> {
    let (tx, rx) = mpsc::channel(1);
    tokio::spawn(async move {
        let _ = tx.send(Err(anyhow::anyhow!(msg))).await;
    });
    rx
}

/// 当前使用的引擎名称（写进历史记录，方便回看时知道是谁答的）
fn engine_model(state: &AppState) -> Option<String> {
    if state.ai.is_configured() {
        Some(state.ai_config.get().model)
    } else if state.mx().is_configured() {
        Some("妙想".to_string())
    } else {
        None
    }
}


// ── 妙想分支的具体问法 ─────────────────────────────────────────
//
// 妙想是「自然语言问答」接口，没有 system prompt 的概念，所以提示词要写进问题里。
// 返回的 markdown 直接交给前端渲染。

fn mx_to_err(e: analyzer::ai::MxError) -> (StatusCode, Json<ErrorResponse>) {
    let code = match e {
        analyzer::ai::MxError::NotConfigured => StatusCode::SERVICE_UNAVAILABLE,
        analyzer::ai::MxError::Upstream(_) => StatusCode::BAD_GATEWAY,
        analyzer::ai::MxError::Transport(_) => StatusCode::BAD_GATEWAY,
        analyzer::ai::MxError::Empty(_) => StatusCode::BAD_GATEWAY,
    };
    ai_error(code, e.to_string())
}

async fn mx_ask(state: &AppState, question: &str) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    state.mx().ask(question, true).await
        .map(|a| a.content)
        .map_err(mx_to_err)
}

async fn mx_diagnose(state: &AppState, code: &str) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    // 先本地取名字，让妙想的诊断更准（它也能自己查，但省一轮）
    let name = state.fetcher.fetch_quote(code).await.ok().map(|q| q.name);
    let client = state.mx();
    let ans = match &name {
        Some(n) => client.stock_analysis(code, n).await,
        None => {
            client.ask(
                &format!("请对A股 {code} 做一次完整的个股诊断：基本面、技术面、资金面、风险点，用 markdown 分节输出"),
                true,
            )
            .await
        }
    };
    ans.map(|a| a.content).map_err(mx_to_err)
}

async fn mx_market(state: &AppState) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    state
        .mx()
        .ask(
            "请对当前A股市场做一个简短研判：大盘情绪、热点板块、资金动向、后市展望，用 markdown 分节输出",
            true,
        )
        .await
        .map(|a| a.content)
        .map_err(mx_to_err)
}

async fn mx_predict(state: &AppState, code: &str) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    let name = state.fetcher.fetch_quote(code).await.ok().map(|q| q.name);
    let who = match &name {
        Some(n) => format!("{n}({code})"),
        None => format!("A股 {code}"),
    };
    state
        .mx()
        .ask(
            &format!("请分析 {who} 的未来趋势：趋势方向、关键价位、支撑压力、风险提示，并明确说明这只是分析不是投资建议"),
            true,
        )
        .await
        .map(|a| a.content)
        .map_err(mx_to_err)
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
        model: engine_model(&state),
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

#[cfg(test)]
mod tests {
    use super::parse_model_ids;

    #[test]
    fn parses_openai_shape() {
        let body = r#"{"object":"list","data":[
            {"id":"gpt-4o-mini","object":"model"},
            {"id":"deepseek-chat","object":"model"}
        ]}"#;
        let ids = parse_model_ids(body).unwrap();
        assert_eq!(ids, vec!["deepseek-chat", "gpt-4o-mini"]);
    }

    #[test]
    fn parses_models_key_and_bare_array() {
        let vllm = r#"{"models":[{"name":"qwen2.5-7b"}]}"#;
        assert_eq!(parse_model_ids(vllm).unwrap(), vec!["qwen2.5-7b"]);

        let bare = r#"["a-model","b-model"]"#;
        assert_eq!(parse_model_ids(bare).unwrap(), vec!["a-model", "b-model"]);
    }

    #[test]
    fn dedups_and_sorts() {
        let body = r#"{"data":[{"id":"b"},{"id":"a"},{"id":"b"}]}"#;
        assert_eq!(parse_model_ids(body).unwrap(), vec!["a", "b"]);
    }

    #[test]
    fn reports_useful_errors() {
        // 同花顺那类数据 API 回去的是错误信封，不是模型列表
        let err = parse_model_ids(r#"{"code":404,"message":"Route not found"}"#).unwrap_err();
        assert!(err.contains("data/models"));

        let err = parse_model_ids("<html>not json</html>").unwrap_err();
        assert!(err.contains("合法 JSON"));
    }
}
