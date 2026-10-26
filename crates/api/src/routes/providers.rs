//! 数据源配置路由：LLM / 东方财富妙想 / 同花顺 fuyao 的读取、保存与连通性测试。
//!
//! 设计要点：
//! - key 一律**打码回显**（`sk-a****nop`），前端拿不到明文；
//! - 前端把打码值原样回传时，视为「不修改」，保留原 key；
//! - 每个来源都有独立的 `/test` 端点，保存前就能验证，不用先存后试；
//! - 保存即热生效：LLM 重建客户端、妙想丢弃缓存客户端，无需重启容器。

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::providers::{ProviderStatus, Providers};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/providers", get(get_providers).put(put_providers))
        .route("/api/v1/providers/test/{kind}", post(test_provider))
}

#[derive(Serialize)]
struct ProvidersView {
    #[serde(flatten)]
    settings: Providers,
    status: ProviderStatus,
}

async fn get_providers(State(state): State<Arc<AppState>>) -> Json<ProvidersView> {
    Json(ProvidersView {
        settings: state.providers.masked(),
        status: state.providers.status(),
    })
}

async fn put_providers(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Providers>,
) -> Result<Json<ProvidersView>, (StatusCode, String)> {
    // 打码值回传 = 不改这个 key
    let merged = state.providers.merge_masked(body);

    if let Err(e) = state.providers.save(merged.clone()) {
        return Err((StatusCode::BAD_REQUEST, e));
    }

    // LLM 热替换：把新配置灌进 AiService
    let llm = merged.llm.clone();
    state.ai.set_llm(analyzer::ai::LlmClient::new(analyzer::ai::LlmConfig {
        api_key: llm.api_key,
        base_url: llm.base_url,
        model: llm.model,
        ..Default::default()
    }));
    // 妙想客户端按签名自动重建
    state.invalidate_mx();

    tracing::info!("数据源配置已更新（LLM/妙想/同花顺）");
    Ok(Json(ProvidersView {
        settings: state.providers.masked(),
        status: state.providers.status(),
    }))
}

/// 测试中的临时凭证（没填则用已保存的）
#[derive(Deserialize, Default)]
struct TestBody {
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    model: String,
}

#[derive(Serialize)]
struct TestResult {
    ok: bool,
    /// 给人看的一句话结论（页面直接显示）
    message: String,
    /// 延迟毫秒（能测才给）
    #[serde(skip_serializing_if = "Option::is_none")]
    latency_ms: Option<u128>,
}

fn pick(incoming: &str, saved: &str) -> String {
    let t = incoming.trim();
    // 打码值或空 → 用已保存的明文
    if t.is_empty() || t.contains("****") {
        saved.to_string()
    } else {
        t.to_string()
    }
}

async fn test_provider(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(kind): axum::extract::Path<String>,
    Json(body): Json<TestBody>,
) -> Result<Json<TestResult>, (StatusCode, Json<TestResult>)> {
    let saved = state.providers.get();

    match kind.as_str() {
        "llm" => {
            let key = pick(&body.api_key, &saved.llm.api_key);
            let base = if body.base_url.trim().is_empty() {
                saved.llm.base_url.clone()
            } else {
                body.base_url.clone()
            };
            let model = if body.model.trim().is_empty() {
                saved.llm.model.clone()
            } else {
                body.model.clone()
            };
            if key.is_empty() {
                return Err(fail(StatusCode::BAD_REQUEST, "请先填写 API Key"));
            }
            let client = analyzer::ai::LlmClient::new(analyzer::ai::LlmConfig {
                api_key: key,
                base_url: base.clone(),
                model: model.clone(),
                ..Default::default()
            });
            let started = std::time::Instant::now();
            match client.chat(vec![analyzer::ai::ChatMessage {
                role: "user".into(),
                content: "回复 OK 两个字母即可".into(),
            }]).await {
                Ok(text) => Ok(Json(TestResult {
                    ok: true,
                    message: format!(
                        "连接成功，模型 {} 已响应：{}",
                        model,
                        text.chars().take(40).collect::<String>()
                    ),
                    latency_ms: Some(started.elapsed().as_millis()),
                })),
                Err(e) => Err(fail(StatusCode::BAD_GATEWAY, &format!("连接失败：{e}"))),
            }
        }
        "mx" => {
            let key = pick(&body.api_key, &saved.mx.api_key);
            let base = if body.base_url.trim().is_empty() {
                saved.mx.base_url.clone()
            } else {
                body.base_url.clone()
            };
            if key.is_empty() {
                return Err(fail(StatusCode::BAD_REQUEST, "请先填写妙想 API Key"));
            }
            let client = analyzer::ai::MxClient::new(base, key);
            match client.ping().await {
                Ok(ms) => Ok(Json(TestResult {
                    ok: true,
                    message: "妙想连接成功，问答接口可正常返回".into(),
                    latency_ms: Some(ms),
                })),
                Err(e) => Err(fail(StatusCode::BAD_GATEWAY, &format!("{e}"))),
            }
        }
        "fuyao" => {
            let key = pick(&body.api_key, &saved.fuyao.api_key);
            if key.is_empty() {
                return Err(fail(StatusCode::BAD_REQUEST, "请先填写同花顺 API Key"));
            }
            match crate::providers::fuyao_ping(&key).await {
                Ok(ms) => Ok(Json(TestResult {
                    ok: true,
                    message: "同花顺 fuyao 连接成功".into(),
                    latency_ms: Some(ms),
                })),
                Err(e) => Err(fail(StatusCode::BAD_GATEWAY, &format!("{e}"))),
            }
        }
        other => Err(fail(
            StatusCode::BAD_REQUEST,
            &format!("未知数据源：{other}（可选 llm / mx / fuyao）"),
        )),
    }
}

fn fail(code: StatusCode, msg: &str) -> (StatusCode, Json<TestResult>) {
    (
        code,
        Json(TestResult { ok: false, message: msg.to_string(), latency_ms: None }),
    )
}
