//! 东方财富妙想（MXClaw）客户端。
//!
//! 与 OpenAI 兼容网关的关键差异（踩过坑，别照抄 LLM 那套）：
//! 1. 鉴权头是 `em_api_key: <key>`，不是 `Authorization: Bearer`；
//! 2. 端点是自有业务路径，不是 `/chat/completions`；
//! 3. 请求体是 `{"question": ..., "deepThink": ...}` 等业务字段；
//! 4. 响应正文在 `data.displayData`（AI 文本）/ `data.partialResults`（表格）；
//! 5. 上游中文**有时以 GBK 字节被错误解码**，需要按字节回退重解。

use serde::{Deserialize, Serialize};
use std::time::Duration;

const TIMEOUT_SECS: u64 = 90;

/// 业务端点（相对 base_url）
pub const EP_ASK: &str = "/app-robo-advisor-api/assistant/ask";
pub const EP_STOCK_ANALYSIS: &str = "/app-robo-advisor-api/assistant/stock-analysis";
pub const EP_SEARCH_DATA: &str = "/b/mcp/tool/searchData";
pub const EP_SELECT_SECURITY: &str = "/b/mcp/tool/selectSecurity";
pub const EP_SEARCH_NEWS: &str = "/b/mcp/tool/searchNews";

#[derive(Debug, Clone)]
pub struct MxClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

/// 妙想返回的可读结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MxAnswer {
    pub content: String,
    /// 是否有结构化数据（表格/列表）被压进 content
    pub has_table: bool,
}

#[derive(Debug)]
pub enum MxError {
    /// 未配置 key
    NotConfigured,
    /// 网络/上游错误
    Transport(String),
    /// 上游返回了业务错误（如 key 无效、额度用尽）
    Upstream(String),
    /// 返回体里没有可读内容
    Empty(String),
}

impl std::fmt::Display for MxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MxError::NotConfigured => write!(f, "未配置妙想 API Key，请到「设置」页填写"),
            MxError::Transport(e) => write!(f, "妙想请求失败：{e}"),
            MxError::Upstream(e) => write!(f, "妙想返回错误：{e}"),
            MxError::Empty(e) => write!(f, "妙想返回空内容：{e}"),
        }
    }
}

impl std::error::Error for MxError {}

impl MxClient {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(TIMEOUT_SECS))
            .connect_timeout(Duration::from_secs(15))
            .user_agent("stock-buddy/1.0 (+mx-client)")
            .build()
            .unwrap_or_default();
        Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.api_key.trim().is_empty()
    }

    /// 金融 AI 问答
    pub async fn ask(&self, question: &str, deep_think: bool) -> Result<MxAnswer, MxError> {
        let body = serde_json::json!({ "question": question, "deepThink": deep_think });
        let v = self.post(EP_ASK, body).await?;
        Self::extract_ai(&v)
    }

    /// 个股诊断（AI 文本）
    pub async fn stock_analysis(&self, code: &str, name: &str) -> Result<MxAnswer, MxError> {
        let body = serde_json::json!({
            "securityCode": code,
            "securityName": name,
        });
        let v = self.post(EP_STOCK_ANALYSIS, body).await?;
        Self::extract_ai(&v)
    }

    /// 自然语言金融数据查询（可能返回表格）
    pub async fn search_data(&self, query: &str) -> Result<MxAnswer, MxError> {
        let body = serde_json::json!({
            "query": query,
            "entity": "",
            "indicator": "",
            "type": "quant",
        });
        let v = self.post(EP_SEARCH_DATA, body).await?;
        Self::extract_ai(&v)
    }

    /// 自然语言选股（返回 markdown 表格在 partialResults）
    pub async fn select_security(
        &self,
        query: &str,
        market: &str,
        category: &str,
        count: u32,
    ) -> Result<MxAnswer, MxError> {
        let body = serde_json::json!({
            "query": query,
            "market": market,
            "category": category,
            "count": count,
        });
        let v = self.post(EP_SELECT_SECURITY, body).await?;
        // 选股把条件摘要和表格拼起来，比只返回表格有用
        let mut out = Self::extract_ai(&v)?;
        if let Some(data) = v.get("data") {
            let cond = data.get("totalCondition").and_then(|x| x.as_str()).unwrap_or("");
            let cnt = data.get("securityCount").and_then(|x| x.as_i64()).unwrap_or(0);
            if !cond.trim().is_empty() {
                let head = format!("**选股条件**：{}\n**符合条件**：{} 只\n\n", fix_gbk(cond), cnt);
                out.content = format!("{head}{}", out.content);
            }
        }
        Ok(out)
    }

    /// 资讯搜索
    pub async fn search_news(&self, query: &str) -> Result<MxAnswer, MxError> {
        let body = serde_json::json!({ "query": query });
        let v = self.post(EP_SEARCH_NEWS, body).await?;
        Self::extract_ai(&v)
    }

    /// 连通性测试：用一次最轻的问答探路。返回延迟毫秒数。
    pub async fn ping(&self) -> Result<u128, MxError> {
        let started = std::time::Instant::now();
        let ans = self.ask("今天A股大盘怎么样，一句话回答", false).await?;
        if ans.content.trim().is_empty() {
            return Err(MxError::Empty("问答返回为空".into()));
        }
        Ok(started.elapsed().as_millis())
    }

    async fn post(&self, path: &str, body: serde_json::Value) -> Result<serde_json::Value, MxError> {
        if !self.is_configured() {
            return Err(MxError::NotConfigured);
        }
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .header("em_api_key", self.api_key.trim())
            .json(&body)
            .send()
            .await
            .map_err(|e| MxError::Transport(e.to_string()))?;

        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| MxError::Transport(e.to_string()))?;
        // 上游可能回 GBK，先按字节解，别直接 resp.text()
        let text = decode_body(&bytes);
        let v: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| MxError::Transport(format!("响应非 JSON（HTTP {status}）：{e}")))?;

        // 业务层错误：code==401 或 status<0
        let code = v.get("code").and_then(|c| c.as_i64());
        let st = v.get("status").and_then(|c| c.as_i64());
        if code == Some(401) || st.map(|s| s < 0).unwrap_or(false) {
            let msg = v
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("未知错误");
            return Err(MxError::Upstream(fix_gbk(msg)));
        }
        if !status.is_success() {
            return Err(MxError::Transport(format!("HTTP {status}")));
        }
        Ok(v)
    }

    /// 按优先级取可读文本：displayData → content/answer/summary → partialResults
    fn extract_ai(v: &serde_json::Value) -> Result<MxAnswer, MxError> {
        let data = v.get("data").ok_or_else(|| MxError::Empty("缺少 data 字段".into()))?;

        let mut table: Option<String> = None;
        if let Some(dd) = data.get("displayData") {
            match dd {
                serde_json::Value::String(s) if !s.trim().is_empty() => {
                    return Ok(MxAnswer { content: fix_gbk(s.trim()), has_table: false });
                }
                serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                    table = Some(
                        serde_json::to_string_pretty(dd).unwrap_or_else(|_| "[]".to_string()),
                    );
                }
                _ => {}
            }
        }
        if table.is_none() {
            for k in ["partialResults", "content", "answer", "summary"] {
                if let Some(serde_json::Value::String(s)) = data.get(k) {
                    if !s.trim().is_empty() {
                        table = Some(fix_gbk(s.trim()));
                        break;
                    }
                }
            }
        }
        match table {
            Some(content) => {
                let has_table = content.contains('|') || content.starts_with('[');
                Ok(MxAnswer { content, has_table })
            }
            None => Err(MxError::Empty("data 中无可读字段".into())),
        }
    }
}

/// 先按 UTF-8 解；失败或明显乱码时按 GBK 重解。
fn decode_body(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => {
            // 极端情况：服务端声明 UTF-8 但实际塞了 GBK（会出替换符）
            if s.contains('\u{FFFD}') {
                fix_gbk(s)
            } else {
                s.to_string()
            }
        }
        Err(_) => {
            let (cow, _, _) = encoding_rs::GBK.decode(bytes);
            cow.into_owned()
        }
    }
}

/// 修复「GBK 字节被当成 latin-1 解码」造成的乱码。
fn fix_gbk(s: &str) -> String {
    // 模式 A：GBK 字节被当作 **latin-1** 解码 → 每个字节变一个 U+0080..U+00FF 字符。
    // 这种可以直接还原回原始字节。
    let all_latin1 = !s.is_empty()
        && s.chars().all(|c| {
            let o = c as u32;
            (0x80..=0xFF).contains(&o) || o < 0x80
        })
        && s.chars().any(|c| (c as u32) >= 0x80);
    if all_latin1 {
        let bytes: Vec<u8> = s.chars().map(|c| c as u32 as u8).collect();
        let (cow, _, had_err) = encoding_rs::GBK.decode(&bytes);
        if !had_err {
            return cow.into_owned();
        }
    }

    // 模式 B：字节被当作 **GBK** 解码了一遍 → 原始 GBK 字节先编码回来再按 UTF-8 解。
    // 只在「多数字符都落在 CJK/全角区」时尝试，避免误伤正常中文。
    let cjk = s
        .chars()
        .filter(|c| (0x4E00..=0x9FFF).contains(&(*c as u32)))
        .count();
    let total = s.chars().count();
    if total > 0 && cjk * 2 > total {
        let (bytes, _, enc_err) = encoding_rs::GBK.encode(s);
        if !enc_err {
            if let Ok(fixed) = std::str::from_utf8(&bytes) {
                if fixed != s {
                    return fixed.to_string();
                }
            }
        }
    }

    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_display_data() {
        let v = serde_json::json!({
            "code": 0,
            "data": { "displayData": "  今天大盘放量上涨。  " }
        });
        let a = MxClient::extract_ai(&v).unwrap();
        assert_eq!(a.content, "今天大盘放量上涨。");
        assert!(!a.has_table);
    }

    #[test]
    fn extracts_partial_results_table() {
        let v = serde_json::json!({
            "data": { "partialResults": "| 代码 | 名称 |\n|---|---|\n| 600519 | 贵州茅台 |" }
        });
        let a = MxClient::extract_ai(&v).unwrap();
        assert!(a.has_table);
        assert!(a.content.contains("600519"));
    }

    #[test]
    fn errors_on_missing_data() {
        let v = serde_json::json!({ "code": 0 });
        assert!(matches!(MxClient::extract_ai(&v), Err(MxError::Empty(_))));
    }

    #[test]
    fn errors_on_empty_display_data() {
        let v = serde_json::json!({ "data": { "displayData": "   " } });
        assert!(matches!(MxClient::extract_ai(&v), Err(MxError::Empty(_))));
    }

    #[test]
    fn not_configured_rejects_without_network() {
        let c = MxClient::new(default_base(), "");
        assert!(!c.is_configured());
        // 没 key 不该发请求，应立刻返回 NotConfigured
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let res = rt.block_on(c.ask("x", false));
        assert!(matches!(res, Err(MxError::NotConfigured)));
    }

    #[test]
    fn gbk_garbled_text_is_repaired() {
        // 构造真实的乱码场景：GBK 字节被当成 latin-1 解码
        // 模式 A：GBK 字节被当成 latin-1 解码
        let (gbk_bytes, _, _) = encoding_rs::GBK.encode("股票行情");
        let latin1_broken: String = gbk_bytes.iter().map(|b| char::from(*b)).collect();
        assert_eq!(fix_gbk(&latin1_broken), "股票行情");

        // 模式 B：UTF-8 字节被当成 GBK 解码（国内接口最常见的乱码形态）
        let utf8_bytes = "涨停".as_bytes();
        let (gbk_broken, _, had_err) = encoding_rs::GBK.decode(utf8_bytes);
        assert!(!had_err);
        assert_ne!(gbk_broken.as_ref(), "涨停");
        assert_eq!(fix_gbk(gbk_broken.as_ref()), "涨停");
    }

    #[test]
    fn plain_ascii_untouched() {
        assert_eq!(fix_gbk("AAPL 123"), "AAPL 123");
    }

    fn default_base() -> String {
        "https://ai-saas.eastmoney.com/proxy".to_string()
    }
}
