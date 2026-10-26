//! 多数据源配置中心：LLM / 东方财富妙想 / 同花顺 fuyao。
//!
//! 取代原先只放 LLM 的 `ai-config.json`。三者都是「用户的私有凭证」，
//! 都必须在页面可填、可测、保存即热生效，且 key 一律打码回显。
//!
//! 兼容旧文件：启动时若只有 `ai-config.json`，会自动迁移进 providers.json 的
//! `llm` 段，老用户升级后不用重填。
//!
//! 优先级：运行时配置 > 环境变量 > 默认值。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// LLM（OpenAI 兼容网关）配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmSettings {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

/// 东方财富妙想（MXClaw）配置。
///
/// 注意它**不是 OpenAI 兼容协议**：鉴权头是 `em_api_key`，端点是自有路径，
/// 响应里是 `data.displayData` 而不是 `choices[0].message.content`。
/// 因此需要单独的 base_url 与 key，不能和 LLM 那段混用。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MxSettings {
    pub api_key: String,
    #[serde(default = "default_mx_base")]
    pub base_url: String,
}

fn default_mx_base() -> String {
    "https://ai-saas.eastmoney.com/proxy".to_string()
}

/// 同花顺金融数据 fuyao 配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FuyaoSettings {
    pub api_key: String,
}

/// 全部数据源配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Providers {
    #[serde(default)]
    pub llm: LlmSettings,
    #[serde(default)]
    pub mx: MxSettings,
    #[serde(default)]
    pub fuyao: FuyaoSettings,
}

impl Providers {
    fn from_env() -> Self {
        Self {
            llm: LlmSettings {
                api_key: std::env::var("LLM_API_KEY").unwrap_or_default(),
                base_url: std::env::var("LLM_BASE_URL")
                    .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
                model: std::env::var("LLM_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string()),
            },
            mx: MxSettings {
                api_key: std::env::var("EM_API_KEY").unwrap_or_default(),
                base_url: std::env::var("EM_BASE_URL").unwrap_or_else(|_| default_mx_base()),
            },
            fuyao: FuyaoSettings {
                api_key: std::env::var("FUYAO_API_KEY").unwrap_or_default(),
            },
        }
    }
}

/// 保存时用的校验（只在「用户主动保存」时跑，允许部分留空）
fn validate(settings: &Providers) -> Result<(), String> {
    if !settings.llm.base_url.trim().is_empty()
        && !(settings.llm.base_url.starts_with("http://")
            || settings.llm.base_url.starts_with("https://"))
    {
        return Err("LLM Base URL 必须以 http:// 或 https:// 开头".into());
    }
    if !settings.mx.base_url.trim().is_empty()
        && !(settings.mx.base_url.starts_with("http://")
            || settings.mx.base_url.starts_with("https://"))
    {
        return Err("妙想 Base URL 必须以 http:// 或 https:// 开头".into());
    }
    Ok(())
}

/// 全局配置（进程内单例，带文件持久化）
pub struct ProviderStore {
    path: PathBuf,
    current: RwLock<Providers>,
}

impl ProviderStore {
    /// 从 `data_dir/providers.json` 加载；并兼容迁移旧的 `ai-config.json`。
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("providers.json");
        let current = match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str::<Providers>(&s).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "providers.json 解析失败，回退环境变量");
                Providers::from_env()
            }),
            Err(_) => {
                // 旧版只有 ai-config.json，把 LLM 段搬过来，避免老用户重填
                let legacy = data_dir.join("ai-config.json");
                let mut p = Providers::from_env();
                if let Ok(s) = std::fs::read_to_string(&legacy) {
                    if let Ok(old) = serde_json::from_str::<LlmSettings>(&s) {
                        tracing::info!("检测到旧版 ai-config.json，已迁移为 providers.json 的 llm 段");
                        p.llm = old;
                    }
                }
                p
            }
        };
        Self { path, current: RwLock::new(current) }
    }

    pub fn get(&self) -> Providers {
        self.current.read().map(|g| g.clone()).unwrap_or_default()
    }

    /// 保存并立即生效（临时文件 + rename，避免写一半损坏配置）
    pub fn save(&self, settings: Providers) -> Result<(), String> {
        validate(&settings)?;
        let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| format!("写入配置失败: {e}"))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| format!("保存配置失败: {e}"))?;
        if let Ok(mut w) = self.current.write() {
            *w = settings;
        }
        Ok(())
    }

    /// 对外展示：所有 key 打码，只看是否已配置。
    pub fn masked(&self) -> Providers {
        let p = self.get();
        Providers {
            llm: LlmSettings {
                api_key: mask_key(&p.llm.api_key),
                base_url: p.llm.base_url,
                model: p.llm.model,
            },
            mx: MxSettings {
                api_key: mask_key(&p.mx.api_key),
                base_url: p.mx.base_url,
            },
            fuyao: FuyaoSettings {
                api_key: mask_key(&p.fuyao.api_key),
            },
        }
    }

    /// 各数据源是否已配置（页面据此显示徽标）
    pub fn status(&self) -> ProviderStatus {
        let p = self.get();
        ProviderStatus {
            llm: !p.llm.api_key.trim().is_empty(),
            mx: !p.mx.api_key.trim().is_empty(),
            fuyao: !p.fuyao.api_key.trim().is_empty(),
        }
    }

    /// 打码值回传时表示「不修改该 key」
    pub fn merge_masked(&self, mut incoming: Providers) -> Providers {
        let cur = self.get();
        if incoming.llm.api_key.contains("****") {
            incoming.llm.api_key = cur.llm.api_key;
        }
        if incoming.mx.api_key.contains("****") {
            incoming.mx.api_key = cur.mx.api_key;
        }
        if incoming.fuyao.api_key.contains("****") {
            incoming.fuyao.api_key = cur.fuyao.api_key;
        }
        incoming
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ProviderStatus {
    pub llm: bool,
    pub mx: bool,
    pub fuyao: bool,
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
    fn masks_all_three_sources() {
        assert_eq!(mask_key(""), "");
        assert_eq!(mask_key("abc"), "****");
        assert_eq!(mask_key("sk-abcdefghijklmnop"), "sk-a****nop");
        // 妙想的 key 形态也照样处理
        assert_eq!(mask_key("em-1234567890abcdef"), "em-1****def");
    }

    #[test]
    fn masked_key_is_preserved_on_save() {
        let dir = std::env::temp_dir().join(format!("sb-provider-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = ProviderStore::load(&dir);
        store
            .save(Providers {
                llm: LlmSettings { api_key: "real-llm-key".into(), ..Default::default() },
                mx: MxSettings { api_key: "real-mx-key".into(), ..Default::default() },
                fuyao: FuyaoSettings { api_key: "real-fuyao-key".into() },
            })
            .unwrap();

        // 前端把打码值回传 → 原 key 不能被覆盖成字面量 "****"
        let incoming = store.masked();
        let merged = store.merge_masked(incoming);
        assert_eq!(merged.llm.api_key, "real-llm-key");
        assert_eq!(merged.mx.api_key, "real-mx-key");
        assert_eq!(merged.fuyao.api_key, "real-fuyao-key");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn status_reflects_configured_keys() {
        let dir = std::env::temp_dir().join(format!("sb-provider-st-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = ProviderStore::load(&dir);
        store
            .save(Providers {
                llm: LlmSettings { api_key: "k".into(), ..Default::default() },
                mx: MxSettings::default(),
                fuyao: FuyaoSettings::default(),
            })
            .unwrap();
        let st = store.status();
        assert!(st.llm);
        assert!(!st.mx);
        assert!(!st.fuyao);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_bad_base_url() {
        let bad = Providers {
            llm: LlmSettings { base_url: "api.openai.com".into(), ..Default::default() },
            ..Default::default()
        };
        assert!(validate(&bad).is_err());
    }

    #[test]
    fn mx_default_base_is_eastmoney_proxy() {
        assert_eq!(default_mx_base(), "https://ai-saas.eastmoney.com/proxy");
    }
}

/// 同花顺 fuyao 连通性测试：打一次最轻的接口（涨停池），能解析就算通。
pub async fn fuyao_ping(api_key: &str) -> Result<u128, String> {
    let f = fetcher::fuyao::FuyaoFetcher::with_api_key(api_key);
    let started = std::time::Instant::now();
    match f.fetch_limit_ups().await {
        Ok(rows) => {
            let ms = started.elapsed().as_millis();
            tracing::info!(rows = rows.len(), ms, "同花顺 fuyao 连通性测试通过");
            Ok(ms)
        }
        Err(e) => Err(format!("同花顺请求失败：{e}")),
    }
}
