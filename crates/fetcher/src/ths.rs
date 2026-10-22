use appcore::models::ThemeStock;
use appcore::{AppError, AppResult};
use reqwest::Client;
use serde::Deserialize;

/// 同花顺数据中心
const THS_BASE: &str = "http://zx.10jqka.com.cn/event/api";

/// 同花顺数据采集器
pub struct ThsFetcher {
    client: Client,
}

impl ThsFetcher {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
            .build()
            .expect("Failed to create HTTP client");
        Self { client }
    }

    /// 获取指定交易日的强势股及其题材归因
    ///
    /// 接口路径末端的 `charset/GBK` 只是同花顺的历史路由约定，**实际响应体是
    /// ASCII 的 JSON**（中文以 `\uXXXX` 形式转义），因此可直接交给 serde_json，
    /// 不需要做 GBK 解码。这一点容易误判，特地记录。
    ///
    /// 返回的 `reason` 为多个题材以「+」连接的字符串，如
    /// `数据中心+干式变压器+电网设备+控制权变更预期`，由上层按「+」拆分做词频统计。
    pub async fn fetch_hot_themes(&self, date: &str) -> AppResult<Vec<ThemeStock>> {
        let url = format!(
            "{}/getharden/date/{}/orderby/date/orderway/desc/charset/GBK/",
            THS_BASE, date
        );

        tracing::debug!("Fetching THS hot themes: {}", url);

        #[derive(Deserialize)]
        struct ThsResp {
            #[serde(default)]
            errocode: Option<i32>,
            #[serde(default)]
            errormsg: Option<String>,
            data: Option<Vec<ThsItem>>,
        }

        #[derive(Deserialize)]
        struct ThsItem {
            #[serde(default)]
            name: String,
            #[serde(default)]
            code: String,
            #[serde(default)]
            reason: String,
            /// 涨跌幅 (%)。注意同花顺另有 `zhangdie` 字段，那是**涨跌额（元）**，
            /// 与 `zhangfu` 含义不同，取错会导致涨跌幅被当成几毛钱。
            #[serde(default)]
            zhangfu: Option<f64>,
        }

        let resp: ThsResp = self.client.get(&url).send().await?.json().await?;

        if let Some(code) = resp.errocode {
            if code != 0 {
                return Err(AppError::Network(format!(
                    "同花顺接口返回错误 code={} msg={}",
                    code,
                    resp.errormsg.unwrap_or_default()
                )));
            }
        }

        let items = resp.data.unwrap_or_default();

        let stocks: Vec<ThemeStock> = items
            .into_iter()
            .filter(|it| !it.code.is_empty() && !it.reason.is_empty())
            .map(|it| ThemeStock {
                code: it.code,
                name: it.name,
                pct_change: it.zhangfu.unwrap_or(0.0),
                reason: it.reason,
            })
            .collect();

        Ok(stocks)
    }
}

impl Default for ThsFetcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ths_payload() {
        // 真实响应片段（中文为 \uXXXX 转义，属 ASCII 安全内容）
        let raw = r#"{"errocode":0,"errormsg":"","data":[{"id":92110487,"name":"顺钠股份","code":"000533","reason":"数据中心+干式变压器","date":"2026-07-24","close":9.55,"zhangdie":0.87,"zhangfu":10.023}]}"#;

        #[derive(Deserialize)]
        struct ThsResp {
            #[serde(default)]
            errocode: Option<i32>,
            data: Option<Vec<ThsItem>>,
        }
        #[derive(Deserialize)]
        struct ThsItem {
            #[serde(default)]
            name: String,
            #[serde(default)]
            code: String,
            #[serde(default)]
            reason: String,
            #[serde(default)]
            zhangfu: Option<f64>,
        }

        let resp: ThsResp = serde_json::from_str(raw).unwrap();
        assert_eq!(resp.errocode, Some(0));
        let item = &resp.data.unwrap()[0];
        assert_eq!(item.name, "顺钠股份");
        assert_eq!(item.code, "000533");
        // zhangfu 是涨跌幅(%)，不是 zhangdie(涨跌额 0.87 元)
        assert!((item.zhangfu.unwrap() - 10.023).abs() < 1e-6);
        assert_eq!(item.reason, "数据中心+干式变压器");
    }
}
