use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc;

/// LLM 配置
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub temperature: f64,
    pub max_tokens: u32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            api_key: std::env::var("LLM_API_KEY").unwrap_or_default(),
            base_url: std::env::var("LLM_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            model: std::env::var("LLM_MODEL")
                .unwrap_or_else(|_| "gpt-4o-mini".to_string()),
            temperature: 0.7,
            max_tokens: 4096,
        }
    }
}

/// Chat 消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".into(), content: content.into() }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".into(), content: content.into() }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: content.into() }
    }
}

/// LLM 请求体
#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f64,
    max_tokens: u32,
    stream: bool,
}

/// LLM 响应体
#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChatMessage,
}

/// SSE 流式响应
#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: Delta,
}

#[derive(Debug, Deserialize)]
struct Delta {
    content: Option<String>,
}

/// LLM 客户端
pub struct LlmClient {
    config: LlmConfig,
    http: reqwest::Client,
}

impl LlmClient {
    pub fn new(config: LlmConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("Failed to build HTTP client");
        Self { config, http }
    }

    pub fn from_env() -> Self {
        Self::new(LlmConfig::default())
    }

    pub fn is_configured(&self) -> bool {
        !self.config.api_key.is_empty()
    }

    /// 同步请求（等待完整响应）
    pub async fn chat(&self, messages: Vec<ChatMessage>) -> anyhow::Result<String> {
        let req = ChatRequest {
            model: self.config.model.clone(),
            messages,
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
            stream: false,
        };

        let resp = self
            .http
            .post(format!("{}/chat/completions", self.config.base_url))
            .bearer_auth(&self.config.api_key)
            .json(&req)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("LLM API error {status}: {body}");
        }

        let chat_resp: ChatResponse = resp.json().await?;
        chat_resp
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| anyhow::anyhow!("LLM returned no choices"))
    }

    /// 流式请求（通过 channel 发送 token）
    pub async fn chat_stream(
        &self,
        messages: Vec<ChatMessage>,
    ) -> anyhow::Result<mpsc::Receiver<Result<String, anyhow::Error>>> {
        let req = ChatRequest {
            model: self.config.model.clone(),
            messages,
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
            stream: true,
        };

        let resp = self
            .http
            .post(format!("{}/chat/completions", self.config.base_url))
            .bearer_auth(&self.config.api_key)
            .json(&req)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("LLM API error {status}: {body}");
        }

        let (tx, rx) = mpsc::channel::<Result<String, anyhow::Error>>(32);

        let http = self.http.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            use futures::StreamExt;

            let mut stream = resp.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));

                        let mut start = 0;
                        while let Some(pos) = buffer[start..].find('\n') {
                            let line = &buffer[start..start + pos];
                            let line = line.trim();

                            if !line.is_empty() && !line.starts_with(':') {
                                if let Some(data) = line.strip_prefix("data: ") {
                                    if data != "[DONE]" {
                                        if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                                            if let Some(choice) = chunk.choices.into_iter().next() {
                                                if let Some(content) = choice.delta.content {
                                                    if !content.is_empty() {
                                                        if tx.send(Ok(content)).await.is_err() {
                                                            return;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            start = start + pos + 1;
                        }

                        if start > 0 {
                            buffer = buffer[start..].to_string();
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(anyhow::anyhow!("Stream error: {e}"))).await;
                        return;
                    }
                }
            }

            let _ = http; // keep alive
            let _ = config;
        });

        Ok(rx)
    }
}
