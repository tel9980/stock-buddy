//! 新浪板块行情备选源。
//!
//! 东财 `push2` 集群的 `clist/get`（板块列表 / 板块资金流）在出口被风控时会**整段
//! 不可达**（返回空响应），而 `stock/get` 却正常。此时东财没有任何可用的板块
//! 列表接口，整块「板块快照 / 板块涨幅榜 / 板块资金流」功能会一起失效。
//!
//! 新浪的这个接口是同口径的替代：一次返回全部行业（或概念）板块的
//! 平均价、涨跌额、涨跌幅、成交量、成交额、领涨股与涨跌幅。
//!
//! ## 接口与编码
//!
//! ```text
//! 行业: http://vip.stock.finance.sina.com.cn/q/view/newSinaHy.php
//! 概念: http://vip.stock.finance.sina.com.cn/q/view/newFLJK.php?param=class
//! ```
//!
//! 响应是 `var S_Finance_bankuai_xxx = {"key":"a,b,c,..."}` 的 **GBK** 文本
//! （必须按字节解码，直接当 UTF-8 读会把板块名全变成乱码），字段位（实测）：
//!
//! | idx | 含义 |
//! |-----|------|
//! | 0 | 板块代码（`new_blhy` / `gn_hwqc`） |
//! | 1 | 板块名称 |
//! | 2 | 成分股数量 |
//! | 3 | 平均价 |
//! | 4 | 涨跌额 |
//! | 5 | **涨跌幅 (%)** |
//! | 6 | 成交量（股） |
//! | 7 | **成交额（元）** |
//! | 8 | 领涨股代码（带 sh/sz 前缀） |
//! | 9 | 领涨股涨跌幅 (%) |
//! | 10 | 领涨股最新价 |
//! | 11 | 领涨股涨跌额 |
//! | 12 | 领涨股名称 |
//!
//! ## 已知取舍
//!
//! 新浪不给板块成分股涨跌家数，也不给主力资金分档，所以：
//! * `up_count / down_count / flat_count` 返回 0（东财路径才有真值）
//! * 板块资金流在备选源下无法提供，`fetch_sector_capital_flows` 会返回空表

use appcore::{AppError, AppResult};
use reqwest::Client;

/// 备选源的一个板块条目
#[derive(Debug, Clone)]
pub struct SinaBoard {
    pub code: String,
    pub name: String,
    pub change_pct: f64,
    /// 成交额（元）
    pub amount_yuan: f64,
    /// 领涨股代码（已去掉 sh/sz 前缀，纯 6 位）
    pub leader_code: Option<String>,
    pub leader_name: Option<String>,
    pub leader_pct: Option<f64>,
}

/// 抓取全部行业板块
pub async fn fetch_industry_boards(client: &Client) -> AppResult<Vec<SinaBoard>> {
    fetch_boards(client, "http://vip.stock.finance.sina.com.cn/q/view/newSinaHy.php").await
}

/// 抓取全部概念板块
pub async fn fetch_concept_boards(client: &Client) -> AppResult<Vec<SinaBoard>> {
    fetch_boards(
        client,
        "http://vip.stock.finance.sina.com.cn/q/view/newFLJK.php?param=class",
    )
    .await
}

async fn fetch_boards(client: &Client, url: &str) -> AppResult<Vec<SinaBoard>> {
    let resp = client
        .get(url)
        .headers(crate::sina::browser_headers_pub("https://finance.sina.com.cn"))
        .send()
        .await?
        .error_for_status()?;

    // 必须按字节解 GBK：当 UTF-8 读会让所有板块名变成 U+FFFD
    let bytes = resp.bytes().await?;
    let (text, _, had_errors) = encoding_rs::GBK.decode(&bytes);
    if had_errors {
        tracing::debug!("sina board response had GBK decoding errors (partial)");
    }
    parse_boards(&text)
}

/// 从 `var X = {...};` 文本解析出板块条目。
///
/// 抽成独立函数便于单测——线上出问题时最常见的失败就是「字段顺序变了」，
/// 这里用 `split(',')` 取位，长度不足的行会被跳过而不是 panic。
pub fn parse_boards(text: &str) -> AppResult<Vec<SinaBoard>> {
    let start = text
        .find('{')
        .ok_or_else(|| AppError::Parse("sina board: 响应里没有 JSON 对象".into()))?;
    let end = text
        .rfind('}')
        .ok_or_else(|| AppError::Parse("sina board: JSON 对象未闭合".into()))?;
    let json = text
        .get(start..=end)
        .ok_or_else(|| AppError::Parse("sina board: 切片越界".into()))?;

    let map: std::collections::HashMap<String, String> = serde_json::from_str(json)
        .map_err(|e| AppError::Parse(format!("sina board: JSON 解析失败: {e}")))?;

    let mut out = Vec::with_capacity(map.len());
    for raw in map.values() {
        let f: Vec<&str> = raw.split(',').collect();
        // 至少要有到涨跌幅、成交额、领涨股这几段，短行直接跳过
        if f.len() < 13 {
            tracing::debug!(fields = f.len(), "sina board: 字段数不足，跳过该行");
            continue;
        }
        let name = f[1].trim();
        if name.is_empty() {
            continue;
        }
        out.push(SinaBoard {
            code: f[0].trim().to_string(),
            name: name.to_string(),
            change_pct: f[5].trim().parse().unwrap_or(0.0),
            amount_yuan: f[7].trim().parse().unwrap_or(0.0),
            leader_code: strip_market_prefix(f[8].trim()),
            // f[12] 是领涨股名称；领涨股涨跌幅在 f[9]
            leader_name: non_empty(f[12].trim()),
            leader_pct: f[9].trim().parse().ok(),
        });
    }

    if out.is_empty() {
        return Err(AppError::Parse("sina board: 未解析出任何板块（字段布局可能已变）".into()));
    }
    Ok(out)
}

/// `sh603021` → `603021`；不带前缀则原样返回
fn strip_market_prefix(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let core = t
        .strip_prefix("sh")
        .or_else(|| t.strip_prefix("sz"))
        .or_else(|| t.strip_prefix("bj"))
        .unwrap_or(t);
    non_empty(core)
}

fn non_empty(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"var S_Finance_bankuai_sinaindustry = {"new_blhy":"new_blhy,玻璃行业,19,14.387894736842,-0.39105263157895,-2.6460113960114,414371907,7020327419,sh603021,10.116,3.810,0.350,*ST华鹏","new_dlhy":"new_dlhy,电力行业,62,7.8316666666667,-0.041,-0.52078922855449,1955362385,16800816658,sh600795,1.884,3.010,0.130,国电电力"};"#;

    #[test]
    fn parses_sample_feed() {
        let boards = parse_boards(SAMPLE).expect("应能解析");
        assert_eq!(boards.len(), 2);

        let glass = boards.iter().find(|b| b.name == "玻璃行业").unwrap();
        assert_eq!(glass.code, "new_blhy");
        assert!((glass.change_pct - (-2.6460113960114)).abs() < 1e-9);
        assert!((glass.amount_yuan - 7_020_327_419.0).abs() < 1.0);
        assert_eq!(glass.leader_code.as_deref(), Some("603021"));
        assert_eq!(glass.leader_name.as_deref(), Some("*ST华鹏"));
    }

    #[test]
    fn rejects_garbage_and_short_rows() {
        assert!(parse_boards("no json here").is_err());
        // 只有一个 3 字段的残缺行 → 没有可用板块 → 报错而不是返回空
        assert!(parse_boards(r#"var x = {"a":"a,b,c"};"#).is_err());
    }

    #[test]
    fn strips_market_prefix() {
        assert_eq!(strip_market_prefix("sh603021").as_deref(), Some("603021"));
        assert_eq!(strip_market_prefix("sz300291").as_deref(), Some("300291"));
        assert_eq!(strip_market_prefix("bj830799").as_deref(), Some("830799"));
        assert_eq!(strip_market_prefix("600000").as_deref(), Some("600000"));
        assert_eq!(strip_market_prefix("  "), None);
    }

    /// 联调用：需要外网。`cargo test -p fetcher live_fetch -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_fetch_industry() {
        let client = reqwest::Client::new();
        let boards = fetch_industry_boards(&client).await.expect("应能抓到");
        println!("抓到 {} 个行业板块", boards.len());
        for b in boards.iter().take(3) {
            println!("  {} {} {:+.2}% 额={}亿 领涨={:?}", b.code, b.name, b.change_pct, b.amount_yuan / 1e8, b.leader_name);
        }
        // 新浪行业分类目前是 49 个（口径比东财的二级行业粗），只要求数量合理
        assert!(boards.len() >= 40, "实际 {} 个", boards.len());
    }
}
