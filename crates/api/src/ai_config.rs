//! AI（LLM）配置：页面可编辑 + 持久化到 data/ai-config.json + 热生效。
//!
//! 优先级：运行时设置 > 环境变量 > 默认值。
//! 页面上保存后立即对后续请求生效，无需重启容器。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// 可编辑的 AI 配置（不含 temperature/max_tokens 等内部参数）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiSettings {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

impl AiSettings {
    fn from_env() -> Self {
        Self {
            api_key: std::env::var("LLM_API_KEY").unwrap_or_default(),
            base_url: std::env::var("LLM_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            model: std::env::var("LLM_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string()),
        }
    }

    /// 校验：key 非空、base_url 形如 http(s)://
    pub fn validate(&self) -> Result<(), String> {
        if self.api_key.trim().is_empty() {
            return Err("API Key 不能为空".into());
        }
        if !(self.base_url.starts_with("http://") || self.base_url.starts_with("https://")) {
            return Err("Base URL 必须以 http:// 或 https:// 开头".into());
        }
        if self.model.trim().is_empty() {
            return Err("模型名不能为空".into());
        }
        Ok(())
    }
}

/// 全局 AI 配置（进程内单例，带文件持久化）
pub struct AiConfigStore {
    path: PathBuf,
    current: RwLock<AiSettings>,
}

impl AiConfigStore {
    /// 从 `data_dir/ai-config.json` 加载；不存在则回退到环境变量。
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("ai-config.json");
        let current = match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str::<AiSettings>(&s).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "ai-config.json 解析失败，回退环境变量");
                AiSettings::from_env()
            }),
            Err(_) => AiSettings::from_env(),
        };
        Self { path, current: RwLock::new(current) }
    }

    pub fn get(&self) -> AiSettings {
        self.current.read().map(|g| g.clone()).unwrap_or_default()
    }

    /// 保存并立即生效。写入先落临时文件再 rename，避免写一半损坏配置。
    pub fn save(&self, settings: AiSettings) -> Result<(), String> {
        settings.validate()?;
        let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| format!("写入配置失败: {e}"))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| format!("保存配置失败: {e}"))?;
        if let Ok(mut w) = self.current.write() {
            *w = settings;
        }
        Ok(())
    }

    pub fn is_configured(&self) -> bool {
        self.current
            .read()
            .map(|s| !s.api_key.trim().is_empty())
            .unwrap_or(false)
    }

    /// 对外展示用：API Key 打码，只看是否已配置。
    pub fn masked(&self) -> AiSettings {
        let s = self.get();
        AiSettings {
            api_key: mask_key(&s.api_key),
            base_url: s.base_url,
            model: s.model,
        }
    }
}

/// `sk-abcdef123456` → `sk-a****456`；空串保持空。
fn mask_key(key: &str) -> String {
    let k = key.trim();
    if k.is_empty() {
        return String::new();
    }
    let n = k.chars().count();
    if n <= 8 {
        return "****".to_string();
    }
    let head: String = k.chars().take(4).collect();
    let tail: String = k.chars().skip(n - 3).collect();
    format!("{head}****{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_short_and_long() {
        assert_eq!(mask_key(""), "");
        assert_eq!(mask_key("abc"), "****");
        assert_eq!(mask_key("sk-abcdefghijklmnop"), "sk-a****nop");
    }

    #[test]
    fn validate_rejects_bad_input() {
        let mut s = AiSettings {
            api_key: "x".into(),
            base_url: "https://api.openai.com/v1".into(),
            model: "m".into(),
        };
        assert!(s.validate().is_ok());
        s.base_url = "api.openai.com".into();
        assert!(s.validate().is_err());
        s.base_url = "https://api.openai.com/v1".into();
        s.api_key = "  ".into();
        assert!(s.validate().is_err());
    }
}
