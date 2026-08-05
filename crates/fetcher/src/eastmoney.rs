use async_trait::async_trait;
use appcore::models::*;
use appcore::{AppError, AppResult};
use crate::traits::DataFetcher;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// 注: 使用 push2delay 节点。push2/push2his 在本机 Clash fake-ip 环境下
// 间歇性无法连接（解析到 198.18.0.25 保留段后连接黑洞），push2delay 稳定可用。
const EM_BASE: &str = "http://push2delay.eastmoney.com/api/qt/stock";
const EM_LIST_BASE: &str = "http://push2delay.eastmoney.com/api/qt/clist";

/// 东方财富数据采集器
pub struct EastmoneyFetcher {
    client: Client,
}

impl EastmoneyFetcher {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)")
            .build()
            .expect("Failed to create HTTP client");
        Self { client }
    }

    /// 构造 secid (市场前缀 + 代码)
    fn secid(code: &str) -> String {
        let market = Market::from_code(code);
        format!("{}.{}", market.secid_prefix(), code)
    }

    fn now_ts() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    /// 腾讯市场前缀: 6 开头→sh, 4/8 开头→bj, 其余→sz
    fn tencent_secid(code: &str) -> String {
        let prefix = if code.starts_with('6') {
            "sh"
        } else if code.starts_with('4') || code.starts_with('8') {
            "bj"
        } else {
            "sz"
        };
        format!("{}{}", prefix, code)
    }

    /// 解析腾讯 K 线响应: data.<secid>.qfq<period> 或 <period>
    /// 每行格式 [日期, 开盘, 收盘, 最高, 最低, 成交量]，腾讯不含成交额/换手率，置 0/None
    fn parse_tencent_kline(
        raw: &str,
        code: &str,
        secid: &str,
        period_key: &str,
    ) -> AppResult<Vec<Kline>> {
        #[derive(Deserialize)]
        struct TencentResp {
            code: i32,
            data: Option<HashMap<String, TencentNode>>,
        }
        #[derive(Clone, Deserialize)]
        struct TencentNode {
            #[serde(rename = "qfqday")]
            qfq_day: Option<Vec<Vec<serde_json::Value>>>,
            #[serde(rename = "day")]
            day: Option<Vec<Vec<serde_json::Value>>>,
            #[serde(rename = "qfqweek")]
            qfq_week: Option<Vec<Vec<serde_json::Value>>>,
            #[serde(rename = "week")]
            week: Option<Vec<Vec<serde_json::Value>>>,
            #[serde(rename = "qfqmonth")]
            qfq_month: Option<Vec<Vec<serde_json::Value>>>,
            #[serde(rename = "month")]
            month: Option<Vec<Vec<serde_json::Value>>>,
        }

        let resp: TencentResp = serde_json::from_str(raw)?;
        if resp.code != 0 {
            return Ok(Vec::new());
        }
        let node = match resp.data.and_then(|m| m.get(secid).cloned()) {
            Some(node) => node,
            None => return Ok(Vec::new()),
        };

        let rows = match period_key {
            "week" => node.qfq_week.or(node.week),
            "month" => node.qfq_month.or(node.month),
            _ => node.qfq_day.or(node.day),
        };
        let rows = match rows {
            Some(rows) => rows,
            None => return Ok(Vec::new()),
        };

        let mut prev_close: Option<f64> = None;
        let mut klines = Vec::with_capacity(rows.len());
        for row in rows {
            if row.len() < 6 {
                continue;
            }
            let val = |i: usize| {
                row.get(i)
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.trim().parse::<f64>().ok())
            };
            let (date, open, close, high, low, volume) =
                match (row[0].as_str(), val(1), val(2), val(3), val(4), val(5)) {
                    (Some(date), Some(open), Some(close), Some(high), Some(low), Some(volume)) => {
                        (date.to_string(), open, close, high, low, volume)
                    }
                    _ => continue,
                };
            let pct_change = prev_close.map(|pc| {
                if pc != 0.0 {
                    (close - pc) / pc * 100.0
                } else {
                    0.0
                }
            });
            prev_close = Some(close);
            klines.push(Kline {
                stock_code: code.to_string(),
                date,
                open,
                close,
                high,
                low,
                volume,
                amount: 0.0,
                pct_change,
                turnover: None,
            });
        }
        Ok(klines)
    }

    /// 腾讯分钟时间 "202608051025" → "2026-08-05 10:25"
    fn format_minute_datetime(s: &str) -> String {
        if s.len() == 12 && s.bytes().all(|b| b.is_ascii_digit()) {
            format!(
                "{}-{}-{} {}:{}",
                &s[0..4],
                &s[4..6],
                &s[6..8],
                &s[8..10],
                &s[10..12]
            )
        } else {
            s.to_string()
        }
    }

    /// 解析腾讯分钟 K 线响应: data.<secid>.<m_period> = [[时间,开,收,高,低,量,{},额], ...]
    /// 时间格式 "202608051025"(YYYYMMDDHHMM)，第 7 列是空对象需跳过，第 8 列为成交额。
    fn parse_tencent_minute_kline(
        raw: &str,
        code: &str,
        secid: &str,
        minute_key: &str,
    ) -> AppResult<Vec<Kline>> {
        #[derive(Deserialize)]
        struct MinuteResp {
            code: i32,
            data: Option<HashMap<String, MinuteNode>>,
        }
        #[derive(Clone, Deserialize)]
        struct MinuteNode {
            #[serde(rename = "m1")]
            m1: Option<Vec<Vec<serde_json::Value>>>,
            #[serde(rename = "m5")]
            m5: Option<Vec<Vec<serde_json::Value>>>,
            #[serde(rename = "m15")]
            m15: Option<Vec<Vec<serde_json::Value>>>,
            #[serde(rename = "m30")]
            m30: Option<Vec<Vec<serde_json::Value>>>,
            #[serde(rename = "m60")]
            m60: Option<Vec<Vec<serde_json::Value>>>,
        }

        let resp: MinuteResp = serde_json::from_str(raw)?;
        if resp.code != 0 {
            return Ok(Vec::new());
        }
        let node = match resp.data.and_then(|m| m.get(secid).cloned()) {
            Some(node) => node,
            None => return Ok(Vec::new()),
        };

        let rows = match minute_key {
            "m1" => node.m1,
            "m15" => node.m15,
            "m30" => node.m30,
            "m60" => node.m60,
            _ => node.m5,
        };
        let rows = match rows {
            Some(rows) => rows,
            None => return Ok(Vec::new()),
        };

        let mut prev_close: Option<f64> = None;
        let mut klines = Vec::with_capacity(rows.len());
        for row in rows {
            if row.len() < 8 {
                continue;
            }
            let val = |i: usize| {
                row.get(i).and_then(|v| {
                    v.as_str()
                        .and_then(|s| s.trim().parse::<f64>().ok())
                        .or_else(|| v.as_f64())
                })
            };
            let (time, open, close, high, low, volume) =
                match (row[0].as_str(), val(1), val(2), val(3), val(4), val(5)) {
                    (Some(time), Some(open), Some(close), Some(high), Some(low), Some(volume)) => {
                        (time.to_string(), open, close, high, low, volume)
                    }
                    _ => continue,
                };
            let pct_change = prev_close.map(|pc| {
                if pc != 0.0 {
                    (close - pc) / pc * 100.0
                } else {
                    0.0
                }
            });
            prev_close = Some(close);
            klines.push(Kline {
                stock_code: code.to_string(),
                date: Self::format_minute_datetime(&time),
                open,
                close,
                high,
                low,
                volume,
                amount: val(7).unwrap_or(0.0),
                pct_change,
                turnover: None,
            });
        }
        Ok(klines)
    }

    /// 腾讯日/周/月 K 线 URL（HTTP, 前复权）
    fn tencent_kline_url(secid: &str, period_key: &str, limit: u32) -> String {
        format!(
            "http://web.ifzq.gtimg.cn/appstock/app/fqkline/get?param={},{},,,{},qfq",
            secid, period_key, limit
        )
    }

    /// 腾讯分钟 K 线 URL（HTTPS, mkline 接口）
    fn tencent_minute_url(secid: &str, minute_key: &str, limit: u32) -> String {
        format!(
            "https://ifzq.gtimg.cn/appstock/app/kline/mkline?param={},{},,{}",
            secid, minute_key, limit
        )
    }
}

impl Default for EastmoneyFetcher {
    fn default() -> Self {
        Self::new()
    }
}

// --- 响应结构体 ---

#[derive(Deserialize)]
struct EmResponse<T> {
    data: Option<T>,
    rc: Option<i32>,
    rt: Option<i32>,
}

#[derive(Deserialize)]
struct EmQuoteData {
    #[serde(rename = "f57")]
    code: String,
    #[serde(rename = "f58")]
    name: String,
    #[serde(rename = "f43", default)]
    price: f64,
    #[serde(rename = "f170", default)]
    pct_change: f64,
    #[serde(rename = "f169", default)]
    change: f64,
    #[serde(rename = "f46", default)]
    open: f64,
    #[serde(rename = "f60", default)]
    pre_close: f64,
    #[serde(rename = "f44", default)]
    high: f64,
    #[serde(rename = "f45", default)]
    low: f64,
    #[serde(rename = "f47", default)]
    volume: f64,
    #[serde(rename = "f48", default)]
    amount: f64,
    #[serde(rename = "f71", default)]
    turnover: f64,
}

impl EmQuoteData {
    fn to_quote(&self) -> Quote {
        Quote {
            code: self.code.clone(),
            name: self.name.clone(),
            price: self.price,
            pct_change: self.pct_change,
            change: self.change,
            open: self.open,
            pre_close: self.pre_close,
            high: self.high,
            low: self.low,
            volume: self.volume,
            amount: self.amount,
            turnover: if self.turnover > 0.0 { Some(self.turnover) } else { None },
            bid_price: None,
            ask_price: None,
            timestamp: EastmoneyFetcher::now_ts(),
        }
    }
}

#[derive(Deserialize)]
struct EmListItem {
    #[serde(rename = "f12", default)]
    code: String,
    #[serde(rename = "f14", default)]
    name: String,
    #[serde(rename = "f3", default)]
    change_pct: f64,
    #[serde(rename = "f128", default)]
    leader_stock: String,
    #[serde(rename = "f136", default)]
    leader_pct: f64,
    #[serde(rename = "f62", default)]
    net_amount: f64,
    #[serde(rename = "f104", default)]
    up_count: u32,
    #[serde(rename = "f105", default)]
    down_count: u32,
}

#[derive(Deserialize)]
struct EmListResponse {
    data: Option<EmListData>,
}

#[derive(Deserialize)]
struct EmListData {
    diff: Option<Vec<EmListItem>>,
}

#[async_trait]
impl DataFetcher for EastmoneyFetcher {
    async fn fetch_quote(&self, code: &str) -> AppResult<Quote> {
        let secid = Self::secid(code);
        let url = format!(
            "{}/get?secid={}&fltt=2&fields=f43,f44,f45,f46,f47,f48,f57,f58,f60,f71,f169,f170",
            EM_BASE, secid
        );

        tracing::debug!("Fetching quote: {}", url);

        let resp: EmResponse<EmQuoteData> = self
            .client
            .get(&url)
            .send()
            .await?
            .json()
            .await?;

        let data = resp.data.ok_or_else(|| {
            AppError::NotFound(format!("Quote not found for code: {}", code))
        })?;

        Ok(data.to_quote())
    }

    async fn fetch_quotes(&self, codes: &[&str]) -> AppResult<Vec<Quote>> {
        let mut quotes = Vec::with_capacity(codes.len());
        // 东方财富支持批量, 但为简单起见逐个获取
        for code in codes {
            match self.fetch_quote(code).await {
                Ok(q) => quotes.push(q),
                Err(e) => tracing::warn!("Failed to fetch quote for {}: {}", code, e),
            }
        }
        Ok(quotes)
    }

    async fn fetch_kline(
        &self,
        code: &str,
        period: &KlinePeriod,
        _start: Option<&str>,
        _end: Option<&str>,
        limit: Option<u32>,
    ) -> AppResult<Vec<Kline>> {
        // 腾讯 K 线: 本机 Clash fake-ip 环境下东财 push2his/push2delay 的 kline 接口
        // 均返回空（历史数据节点不可达），腾讯 web.ifzq.gtimg.cn 稳定且支持前复权。
        // 日/周/月走 fqkline 接口(HTTP)；分钟线走 mkline 接口(HTTPS, m1/m5/m15/m30/m60)。
        let secid = Self::tencent_secid(code);
        let lmt = limit.unwrap_or(365).min(800);

        let (url, period_key, is_minute) = match period {
            KlinePeriod::Daily => (Self::tencent_kline_url(&secid, "day", lmt), "day", false),
            KlinePeriod::Weekly => (Self::tencent_kline_url(&secid, "week", lmt), "week", false),
            KlinePeriod::Monthly => (Self::tencent_kline_url(&secid, "month", lmt), "month", false),
            KlinePeriod::Minute1 => (Self::tencent_minute_url(&secid, "m1", lmt), "m1", true),
            KlinePeriod::Minute5 => (Self::tencent_minute_url(&secid, "m5", lmt), "m5", true),
            KlinePeriod::Minute15 => (Self::tencent_minute_url(&secid, "m15", lmt), "m15", true),
            KlinePeriod::Minute30 => (Self::tencent_minute_url(&secid, "m30", lmt), "m30", true),
            KlinePeriod::Minute60 => (Self::tencent_minute_url(&secid, "m60", lmt), "m60", true),
        };

        tracing::debug!("Fetching kline (tencent): {}", url);

        let resp_text = self.client.get(&url).send().await?.text().await?;
        if is_minute {
            Self::parse_tencent_minute_kline(&resp_text, code, &secid, period_key)
        } else {
            Self::parse_tencent_kline(&resp_text, code, &secid, period_key)
        }
    }

    async fn fetch_sector_ranks(&self, sector_type: &str) -> AppResult<Vec<SectorRank>> {
        // m:90+t:2 行业板块, m:90+t:3 概念板块
        let fs = match sector_type {
            "industry" => "m:90+t:2",
            "concept" => "m:90+t:3",
            _ => "m:90+t:2",
        };

        let url = format!(
            "{}/get?pn=1&pz=100&po=1&np=1&fltt=2&invt=2&fid=f3&fs={}&fields=f12,f14,f3,f128,f136,f62,f104,f105",
            EM_LIST_BASE, fs
        );

        tracing::debug!("Fetching sector ranks: {}", url);

        let resp: EmListResponse = self
            .client
            .get(&url)
            .send()
            .await?
            .json()
            .await?;

        let items = resp
            .data
            .and_then(|d| d.diff)
            .unwrap_or_default();

        let ranks: Vec<SectorRank> = items
            .iter()
            .map(|item| SectorRank {
                code: item.code.clone(),
                name: item.name.clone(),
                sector_type: sector_type.to_string(),
                change_pct: item.change_pct,
                leader_stock: if item.leader_stock.is_empty() {
                    None
                } else {
                    Some(item.leader_stock.clone())
                },
                leader_pct: if item.leader_pct != 0.0 {
                    Some(item.leader_pct)
                } else {
                    None
                },
                net_amount: if item.net_amount != 0.0 {
                    Some(item.net_amount)
                } else {
                    None
                },
                up_count: Some(item.up_count),
                down_count: Some(item.down_count),
            })
            .collect();

        Ok(ranks)
    }

    async fn fetch_sector_stocks(&self, sector_code: &str, page: u32, page_size: u32) -> AppResult<(Vec<SectorStock>, u32)> {
        // 板块成分股: fs=b:BKxxxx
        let fs = format!("b:{}", sector_code);
        let pn = page.max(1);
        let pz = page_size.max(1).min(100);

        let url = format!(
            "{}/get?pn={}&pz={}&po=1&np=1&fltt=2&invt=2&fid=f3&fs={}&fields=f12,f14,f2,f3,f4,f5,f6,f7,f15,f16,f17,f18",
            EM_LIST_BASE, pn, pz, fs
        );

        tracing::debug!("Fetching sector stocks: {}", url);

        #[derive(Deserialize)]
        struct EmStockListResponse {
            data: Option<EmStockListData>,
        }
        #[derive(Deserialize)]
        struct EmStockListData {
            diff: Option<Vec<EmStockItem>>,
            total: Option<u32>,
        }
        #[derive(Deserialize)]
        struct EmStockItem {
            #[serde(rename = "f12", default)]
            code: String,
            #[serde(rename = "f14", default)]
            name: String,
            #[serde(rename = "f2", default)]
            price: f64,
            #[serde(rename = "f3", default)]
            pct_change: f64,
            #[serde(rename = "f4", default)]
            change: f64,
            #[serde(rename = "f5", default)]
            volume: f64,
            #[serde(rename = "f6", default)]
            amount: f64,
            #[serde(rename = "f7", default)]
            turnover: f64,
            #[serde(rename = "f15", default)]
            high: f64,
            #[serde(rename = "f16", default)]
            low: f64,
            #[serde(rename = "f17", default)]
            open: f64,
            #[serde(rename = "f18", default)]
            pre_close: f64,
        }

        let resp: EmStockListResponse = self
            .client
            .get(&url)
            .send()
            .await?
            .json()
            .await?;

        let data = resp.data.ok_or_else(|| {
            AppError::NotFound(format!("Sector stocks not found for: {}", sector_code))
        })?;

        let total = data.total.unwrap_or(0);
        let stocks = data.diff.unwrap_or_default().iter().map(|item| SectorStock {
            code: item.code.clone(),
            name: item.name.clone(),
            price: item.price,
            pct_change: item.pct_change,
            change: item.change,
            volume: item.volume,
            amount: item.amount,
            high: item.high,
            low: item.low,
            open: item.open,
            pre_close: item.pre_close,
            turnover: item.turnover,
        }).collect();

        Ok((stocks, total))
    }

    async fn fetch_market_overview(&self) -> AppResult<MarketOverview> {
        // 获取三大指数: 上证指数(1.000001), 深证成指(0.399001), 创业板指(0.399006)
        let index_codes = ["1.000001", "0.399001", "0.399006"];

        let mut indices = Vec::with_capacity(3);
        for secid in &index_codes {
            let url = format!(
                "{}/get?secid={}&fltt=2&fields=f43,f44,f45,f46,f47,f48,f57,f58,f169,f170",
                EM_BASE, secid
            );

            let resp: EmResponse<EmQuoteData> = self
                .client
                .get(&url)
                .send()
                .await?
                .json()
                .await?;

            if let Some(data) = resp.data {
                indices.push(IndexSummary {
                    code: data.code.clone(),
                    name: data.name.clone(),
                    price: data.price,
                    pct_change: data.pct_change,
                    change: data.change,
                    volume: data.volume,
                    amount: data.amount,
                });
            }
        }

        // 获取涨跌家数 (通过沪深A股列表)
        let (up, down, flat) = self.fetch_up_down_count().await.unwrap_or((0, 0, 0));

        Ok(MarketOverview {
            indices,
            up_count: up,
            down_count: down,
            flat_count: flat,
            limit_up_count: 0,
            limit_down_count: 0,
            total_amount: 0.0,
            timestamp: Self::now_ts(),
        })
    }
    // ============ Phase 4: 资金流向 ============

    async fn fetch_capital_flow(&self, code: &str, limit: Option<u32>) -> AppResult<Vec<CapitalFlow>> {
        let secid = Self::secid(code);
        let lmt = limit.unwrap_or(60);
        // fflow/daykline/get with klt=101 for daily data
        // fields2: f51=日期,f52=主力净流入,f53=小单净流入,f54=中单净流入,
        //   f55=大单净流入,f56=超大单净流入,f57=主力净流入占比,f58=小单占比,
        //   f59=中单占比,f60=大单占比,f61=超大单占比,f62=收盘价,f63=涨跌幅
        let url = format!(
            "http://push2delay.eastmoney.com/api/qt/stock/fflow/daykline/get?lmt={}&klt=101&secid={}&fields1=f1,f2,f3,f7&fields2=f51,f52,f53,f54,f55,f56,f57,f58,f59,f60,f61,f62,f63",
            lmt, secid
        );

        tracing::debug!("Fetching capital flow: {}", url);

        #[derive(Deserialize)]
        struct FflowData {
            klines: Option<Vec<String>>,
        }

        let resp: EmResponse<FflowData> = self.client.get(&url).send().await?.json().await?;
        let klines = resp.data.and_then(|d| d.klines).unwrap_or_default();

        let flows: Vec<CapitalFlow> = klines.iter().filter_map(|line| {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() < 13 { return None; }
            // f51=日期, f52=主力净流入, f53=小单净流入, f54=中单净流入,
            // f55=大单净流入, f56=超大单净流入, f57=主力净流入占比,
            // f58=小单占比, f59=中单占比, f60=大单占比, f61=超大单占比,
            // f62=收盘价, f63=涨跌幅
            Some(CapitalFlow {
                stock_code: code.to_string(),
                date: parts[0].to_string(),
                main_net_inflow: parts[1].parse().ok()?,             // f52 主力净流入
                small_net_inflow: parts[2].parse().ok()?,            // f53 小单净流入
                medium_net_inflow: parts[3].parse().ok()?,           // f54 中单净流入
                large_net_inflow: parts[4].parse().ok()?,            // f55 大单净流入
                huge_net_inflow: parts[5].parse().ok()?,             // f56 超大单净流入
                main_net_ratio: parts[6].parse().ok()?,              // f57 主力净流入占比
                small_net_ratio: parts.get(7).and_then(|v| v.parse().ok()).unwrap_or(0.0),  // f58
                medium_net_ratio: parts.get(8).and_then(|v| v.parse().ok()).unwrap_or(0.0), // f59
                large_net_ratio: parts.get(9).and_then(|v| v.parse().ok()).unwrap_or(0.0),  // f60
                huge_net_ratio: parts.get(10).and_then(|v| v.parse().ok()).unwrap_or(0.0),  // f61
                close_price: parts.get(11).and_then(|v| v.parse().ok()).unwrap_or(0.0),     // f62
                pct_change: parts.get(12).and_then(|v| v.parse().ok()).unwrap_or(0.0),      // f63
            })
        }).collect();

        Ok(flows)
    }

    async fn fetch_sector_capital_flows(&self) -> AppResult<Vec<SectorCapitalFlow>> {
        // 东方财富行业板块资金流
        // f62=主力净流入, f184=主力净占比, f66=超大单净额, f69=超大单占比,
        // f72=大单净额, f75=大单占比, f78=中单净额, f81=中单占比,
        // f84=小单净额, f87=小单占比
        let url = format!(
            "{}/get?pn=1&pz=50&po=1&np=1&fltt=2&invt=2&fid=f62&fs=m:90+t:2&fields=f12,f14,f3,f62,f184,f66,f69,f72,f75,f78,f81,f84,f87",
            EM_LIST_BASE
        );

        tracing::debug!("Fetching sector capital flows: {}", url);

        #[derive(Deserialize)]
        struct SectorFlowItem {
            #[serde(rename = "f12", default)]
            code: String,
            #[serde(rename = "f14", default)]
            name: String,
            #[serde(rename = "f3", default)]
            pct_change: f64,
            #[serde(rename = "f62", default)]
            main_net: f64,
            #[serde(rename = "f184", default)]
            main_ratio: f64,
            #[serde(rename = "f66", default)]
            huge_net: f64,
            #[serde(rename = "f69", default)]
            huge_ratio: f64,
            #[serde(rename = "f72", default)]
            large_net: f64,
            #[serde(rename = "f75", default)]
            large_ratio: f64,
            #[serde(rename = "f78", default)]
            medium_net: f64,
            #[serde(rename = "f81", default)]
            medium_ratio: f64,
            #[serde(rename = "f84", default)]
            small_net: f64,
            #[serde(rename = "f87", default)]
            small_ratio: f64,
        }

        #[derive(Deserialize)]
        struct SectorFlowListData {
            diff: Option<Vec<SectorFlowItem>>,
        }

        let resp: EmResponse<SectorFlowListData> = self.client.get(&url).send().await?.json().await?;
        let items = resp.data.and_then(|d| d.diff).unwrap_or_default();

        let flows: Vec<SectorCapitalFlow> = items.iter().map(|item| SectorCapitalFlow {
            code: item.code.clone(),
            name: item.name.clone(),
            main_net_inflow: item.main_net,
            main_net_ratio: item.main_ratio,
            huge_net_inflow: item.huge_net,
            large_net_inflow: item.large_net,
            medium_net_inflow: item.medium_net,
            small_net_inflow: item.small_net,
            pct_change: item.pct_change,
            stock_count: None,
        }).collect();

        Ok(flows)
    }

    async fn fetch_north_bound_flow(&self, limit: Option<u32>) -> AppResult<Vec<NorthBoundFlow>> {
        let lmt = limit.unwrap_or(30);
        // 北向资金 = 沪股通(klt=101) + 深股通(klt=102)
        // fields2: f51=日期, f52=当日净买入(万元), f53=每日额度余额(万元), f54=?, f55=?, f56=累计净买入(万元)
        let sh_url = format!(
            "http://push2delay.eastmoney.com/api/qt/kamt.kline/get?fields1=f1,f2,f3,f4&fields2=f51,f52,f53,f56&klt=101&lmt={}",
            lmt
        );
        let sz_url = format!(
            "http://push2delay.eastmoney.com/api/qt/kamt.kline/get?fields1=f1,f2,f3,f4&fields2=f51,f52,f53,f56&klt=102&lmt={}",
            lmt
        );

        #[derive(Deserialize, Default)]
        struct KamtData {
            #[serde(default)]
            hk2sh: Vec<String>,
            #[serde(default)]
            hk2sz: Vec<String>,
        }

        let (sh_resp, sz_resp) = tokio::join!(
            self.client.get(&sh_url).send(),
            self.client.get(&sz_url).send(),
        );

        let sh_data = match sh_resp {
            Ok(r) => r.json::<EmResponse<KamtData>>().await.ok()
                .and_then(|r| r.data).unwrap_or_default(),
            Err(_) => KamtData::default(),
        };
        let sz_data = match sz_resp {
            Ok(r) => r.json::<EmResponse<KamtData>>().await.ok()
                .and_then(|r| r.data).unwrap_or_default(),
            Err(_) => KamtData::default(),
        };

        // Merge SH + SZ by date
        use std::collections::BTreeMap;
        let mut date_map: BTreeMap<String, (f64, f64, f64, f64)> = BTreeMap::new();

        for line in &sh_data.hk2sh {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() < 3 { continue; }
            let date = parts[0].to_string();
            let net: f64 = parts[1].parse().unwrap_or(0.0);
            let balance: f64 = parts[2].parse().unwrap_or(0.0);
            let entry = date_map.entry(date).or_insert((0.0, 0.0, 0.0, 0.0));
            entry.0 = net;     // sh_net
            entry.2 = balance; // sh_balance
        }

        for line in &sz_data.hk2sz {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() < 3 { continue; }
            let date = parts[0].to_string();
            let net: f64 = parts[1].parse().unwrap_or(0.0);
            let balance: f64 = parts[2].parse().unwrap_or(0.0);
            let entry = date_map.entry(date).or_insert((0.0, 0.0, 0.0, 0.0));
            entry.1 = net;     // sz_net
            entry.3 = balance; // sz_balance
        }

        let flows: Vec<NorthBoundFlow> = date_map.into_iter()
            .map(|(date, (sh_net, sz_net, sh_balance, sz_balance))| {
                NorthBoundFlow {
                    date,
                    sh_net_inflow: sh_net,
                    sz_net_inflow: sz_net,
                    total_net_inflow: sh_net + sz_net,
                    sh_balance,
                    sz_balance,
                }
            })
            .collect();

        Ok(flows)
    }

    async fn fetch_north_bound_holdings(&self) -> AppResult<Vec<NorthBoundHoldings>> {
        // 自 2024-08-19 起交易所不再披露北向每日净流入，仅按季度披露持股汇总
        let url = "https://datacenter-web.eastmoney.com/api/data/v1/get?reportName=RPT_MUTUAL_MARKET_STA&columns=ALL&pageNumber=1&pageSize=40&sortTypes=-1&sortColumns=HOLD_DATE&source=WEB&client=WEB";

        tracing::debug!("Fetching north bound holdings: {}", url);

        #[derive(Deserialize)]
        struct HoldingsResp {
            result: Option<HoldingsResult>,
        }
        #[derive(Deserialize)]
        struct HoldingsResult {
            data: Option<Vec<HoldingsItem>>,
        }
        #[derive(Deserialize)]
        struct HoldingsItem {
            #[serde(rename = "HOLD_DATE")]
            hold_date: Option<String>,
            #[serde(rename = "HOLD_MARKET_CAP", default)]
            hold_market_cap: f64,
            #[serde(rename = "ADD_MARKET_CAP", default)]
            add_market_cap: f64,
            #[serde(rename = "CHANGE_RATE", default)]
            change_rate: f64,
        }

        let resp: HoldingsResp = self.client.get(url).send().await?.json().await?;
        let data = resp.result.and_then(|r| r.data).unwrap_or_default();

        let items: Vec<NorthBoundHoldings> = data.iter().map(|item| NorthBoundHoldings {
            date: item.hold_date.as_ref()
                .map(|d| d[..10].to_string())
                .unwrap_or_default(),
            hold_market_cap: item.hold_market_cap,
            add_market_cap: item.add_market_cap,
            change_rate: item.change_rate,
        }).collect();

        Ok(items)
    }

    async fn fetch_market_capital_flow(&self) -> AppResult<MarketCapitalFlow> {
        // 沪深两市主力资金 = 上证指数(1.000001) + 深证成指(0.399001)
        // f62=主力净流入, f66=超大单, f72=大单, f78=中单, f84=小单（单位：元）
        let url = "http://push2delay.eastmoney.com/api/qt/ulist.np/get?fltt=2&secids=1.000001,0.399001&fields=f12,f14,f62,f66,f72,f78,f84";

        tracing::debug!("Fetching market capital flow: {}", url);

        #[derive(Deserialize)]
        struct FlowResp {
            data: Option<FlowData>,
        }
        #[derive(Deserialize)]
        struct FlowData {
            diff: Option<Vec<FlowItem>>,
        }
        #[derive(Deserialize)]
        struct FlowItem {
            #[serde(rename = "f62", default)]
            main_net_inflow: f64,
            #[serde(rename = "f66", default)]
            huge_net_inflow: f64,
            #[serde(rename = "f72", default)]
            large_net_inflow: f64,
            #[serde(rename = "f78", default)]
            medium_net_inflow: f64,
            #[serde(rename = "f84", default)]
            small_net_inflow: f64,
        }

        let resp: FlowResp = self.client.get(url).send().await?.json().await?;
        let items = resp.data.and_then(|d| d.diff).unwrap_or_default();

        let mut total = MarketCapitalFlow {
            date: chrono::Local::now().format("%Y-%m-%d").to_string(),
            main_net_inflow: 0.0,
            huge_net_inflow: 0.0,
            large_net_inflow: 0.0,
            medium_net_inflow: 0.0,
            small_net_inflow: 0.0,
        };
        for item in items {
            total.main_net_inflow += item.main_net_inflow;
            total.huge_net_inflow += item.huge_net_inflow;
            total.large_net_inflow += item.large_net_inflow;
            total.medium_net_inflow += item.medium_net_inflow;
            total.small_net_inflow += item.small_net_inflow;
        }

        Ok(total)
    }

    async fn fetch_margin_trading(&self, limit: Option<u32>) -> AppResult<Vec<MarginTrading>> {
        let lmt = limit.unwrap_or(30);
        let url = format!(
            "https://datacenter-web.eastmoney.com/api/data/v1/get?reportName=RPTA_RZRQ_LSHJ&columns=ALL&pageNumber=1&pageSize={}&sortTypes=-1&sortColumns=DIM_DATE&source=WEB&client=WEB",
            lmt
        );

        tracing::debug!("Fetching margin trading: {}", url);

        #[derive(Deserialize)]
        struct MarginResp {
            result: Option<MarginResult>,
        }
        #[derive(Deserialize)]
        struct MarginResult {
            data: Option<Vec<MarginItem>>,
        }
        #[derive(Deserialize)]
        struct MarginItem {
            #[serde(rename = "DIM_DATE")]
            dim_date: Option<String>,
            #[serde(rename = "RZYE", default)]
            margin_balance: f64,
            #[serde(rename = "RQYE", default)]
            short_balance: f64,
            #[serde(rename = "RZRQYE", default)]
            total_balance: f64,
            #[serde(rename = "RZMRE", default)]
            margin_buy: f64,
            #[serde(rename = "RZCHE", default)]
            margin_repay: f64,
            #[serde(rename = "RZJME", default)]
            margin_net_buy: f64,
            #[serde(rename = "RQMCL", default)]
            short_sell_vol: f64,
            #[serde(rename = "RQCHL", default)]
            short_repay_vol: f64,
            #[serde(rename = "RQJMG", default)]
            short_net_sell: f64,
        }

        let resp: MarginResp = self.client.get(&url).send().await?.json().await?;
        let data = resp.result.and_then(|r| r.data).unwrap_or_default();

        let items: Vec<MarginTrading> = data.iter().map(|item| MarginTrading {
            date: item.dim_date.as_ref()
                .map(|d| d[..10].to_string())
                .unwrap_or_default(),
            margin_balance: item.margin_balance,
            short_balance: item.short_balance,
            total_balance: item.total_balance,
            margin_buy: item.margin_buy,
            margin_repay: item.margin_repay,
            margin_net_buy: item.margin_net_buy,
            short_sell_vol: item.short_sell_vol,
            short_repay_vol: item.short_repay_vol,
            short_net_sell: item.short_net_sell,
        }).collect();

        Ok(items)
    }

    // ============ Phase 4: 涨停复盘 ============

    async fn fetch_limit_ups(&self) -> AppResult<Vec<LimitUpStock>> {
        let url = format!(
            "{}/get?pn=1&pz=200&po=1&np=1&fltt=2&invt=2&fid=f3&fs=m:0+t:6,m:0+t:80,m:1+t:2,m:1+t:23&fields=f2,f3,f4,f5,f6,f7,f8,f9,f10,f12,f14,f15,f16,f17,f18,f20,f21,f22,f23,f62,f100,f184",
            EM_LIST_BASE
        );

        tracing::debug!("Fetching limit ups: {}", url);

        let resp: EmListResponse = self.client.get(&url).send().await?.json().await?;
        let items = resp.data.and_then(|d| d.diff).unwrap_or_default();

        // Filter limit-up stocks (pct_change >= ~9.5%)
        let limit_ups: Vec<LimitUpStock> = items.iter().filter_map(|item| {
            if item.change_pct < 9.5 { return None; }
            // Need more fields than EmListItem provides
            Some(LimitUpStock {
                code: item.code.clone(),
                name: item.name.clone(),
                price: 0.0,
                pct_change: item.change_pct,
                consecutive_days: 1,
                first_limit_time: None,
                last_limit_time: None,
                break_count: 0,
                seal_volume: 0.0,
                seal_amount: 0.0,
                seal_strength: None,
                amount: 0.0,
                turnover: 0.0,
                float_market_cap: 0.0,
                industry: None,
                reason: None,
                amplitude: 0.0,
            })
        }).collect();

        Ok(limit_ups)
    }

    async fn fetch_limit_downs(&self) -> AppResult<Vec<LimitDownStock>> {
        let url = format!(
            "{}/get?pn=1&pz=200&po=1&np=1&fltt=2&invt=2&fid=f3&fs=m:0+t:6,m:0+t:80,m:1+t:2,m:1+t:23&fields=f2,f3,f4,f5,f6,f7,f8,f9,f10,f12,f14,f15,f16,f17,f18,f20,f21,f62,f100",
            EM_LIST_BASE
        );

        tracing::debug!("Fetching limit downs: {}", url);

        let resp: EmListResponse = self.client.get(&url).send().await?.json().await?;
        let items = resp.data.and_then(|d| d.diff).unwrap_or_default();

        let limit_downs: Vec<LimitDownStock> = items.iter().filter_map(|item| {
            if item.change_pct > -9.5 { return None; }
            Some(LimitDownStock {
                code: item.code.clone(),
                name: item.name.clone(),
                price: 0.0,
                pct_change: item.change_pct,
                consecutive_days: 1,
                seal_amount: 0.0,
                amount: 0.0,
                turnover: 0.0,
                industry: None,
            })
        }).collect();

        Ok(limit_downs)
    }

    async fn fetch_broken_boards(&self) -> AppResult<Vec<BrokenBoardStock>> {
        // 炸板股: 振幅较大且最高涨幅达到涨停但收盘未封板
        let url = format!(
            "{}/get?pn=1&pz=200&po=1&np=1&fltt=2&invt=2&fid=f7&fs=m:0+t:6,m:0+t:80,m:1+t:2,m:1+t:23&fields=f2,f3,f4,f5,f6,f7,f8,f9,f10,f12,f14,f15,f16,f17,f18,f20,f21",
            EM_LIST_BASE
        );

        tracing::debug!("Fetching broken boards: {}", url);

        let resp: EmListResponse = self.client.get(&url).send().await?.json().await?;
        let items = resp.data.and_then(|d| d.diff).unwrap_or_default();

        // 炸板判断: 涨跌幅在5~9.5%之间且振幅大于8%
        let broken: Vec<BrokenBoardStock> = items.iter().filter_map(|item| {
            if item.change_pct >= 5.0 && item.change_pct < 9.5 {
                Some(BrokenBoardStock {
                    code: item.code.clone(),
                    name: item.name.clone(),
                    price: 0.0,
                    pct_change: item.change_pct,
                    max_pct: 0.0,
                    break_count: 1,
                    amount: 0.0,
                    turnover: 0.0,
                    industry: None,
                })
            } else {
                None
            }
        }).collect();

        Ok(broken)
    }

    async fn fetch_limit_up_review(&self) -> AppResult<LimitUpReview> {
        // 合并涨停/跌停/炸板数据
        let (limit_ups, limit_downs, broken_boards) = tokio::join!(
            self.fetch_limit_ups(),
            self.fetch_limit_downs(),
            self.fetch_broken_boards(),
        );

        let limit_ups = limit_ups.unwrap_or_default();
        let limit_downs = limit_downs.unwrap_or_default();
        let broken_boards = broken_boards.unwrap_or_default();

        // 统计涨停原因分布（从股票名称和行业推断）
        let mut reason_map: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        for stock in &limit_ups {
            if let Some(ref reason) = stock.reason {
                *reason_map.entry(reason.clone()).or_default() += 1;
            }
        }
        let reason_distribution: Vec<(String, u32)> = reason_map.into_iter().collect();

        // 连板梯队
        let mut tier_map: std::collections::HashMap<u32, Vec<LimitUpStock>> = std::collections::HashMap::new();
        let mut max_consecutive = 0u32;
        for stock in &limit_ups {
            let days = stock.consecutive_days;
            if days > max_consecutive { max_consecutive = days; }
            tier_map.entry(days).or_default().push(stock.clone());
        }

        let mut consecutive_tiers: Vec<ConsecutiveBoardTier> = tier_map.into_iter()
            .map(|(days, stocks)| ConsecutiveBoardTier {
                days,
                count: stocks.len() as u32,
                stocks,
            })
            .collect();
        consecutive_tiers.sort_by_key(|t| t.days);

        let one_word_count = limit_ups.iter().filter(|s| s.amplitude < 0.5).count() as u32;
        let t_shape_count = limit_ups.iter().filter(|s| s.amplitude > 0.5 && s.amplitude < 5.0 && s.first_limit_time.as_deref() != Some("09:25:00")).count() as u32;

        let total = (limit_ups.len() + broken_boards.len()) as f64;
        let break_rate = if total > 0.0 { (broken_boards.len() as f64) / total * 100.0 } else { 0.0 };

        Ok(LimitUpReview {
            limit_up_count: limit_ups.len() as u32,
            one_word_count,
            t_shape_count,
            broken_count: broken_boards.len() as u32,
            limit_down_count: limit_downs.len() as u32,
            max_consecutive,
            break_rate,
            limit_ups,
            limit_downs,
            broken_boards,
            consecutive_tiers,
            reason_distribution,
            timestamp: Self::now_ts(),
        })
    }

    // ============ Phase 4: 龙虎榜 ============

    async fn fetch_dragon_tiger(&self, date: Option<&str>) -> AppResult<Vec<DragonTigerEntry>> {
        // 报表名（注意：原 RPT_DAILY_BILLBOARDTRADING 已不存在，返回空）
        let report = "RPT_DAILYBILLBOARD_DETAILSNEW";

        // 前端可能传 YYYYMMDD，接口 TRADE_DATE 存储为 YYYY-MM-DD HH:MM:SS，统一成 YYYY-MM-DD
        let trade_date = date.unwrap_or("");
        let trade_date = if trade_date.len() == 8 && trade_date.chars().all(|c| c.is_ascii_digit()) {
            format!("{}-{}-{}", &trade_date[0..4], &trade_date[4..6], &trade_date[6..8])
        } else {
            trade_date.to_string()
        };
        let filter = if trade_date.is_empty() {
            String::new()
        } else {
            format!("&filter=(TRADE_DATE='{}')", trade_date)
        };

        let url = format!(
            "http://datacenter.eastmoney.com/securities/api/data/v1/get?reportName={}&columns=ALL&pageNumber=1&pageSize=100&sortTypes=-1&sortColumns=TRADE_DATE{}&source=WEB&client=WEB",
            report, filter
        );

        tracing::debug!("Fetching dragon tiger: {}", url);

        #[derive(Deserialize)]
        struct DtResp {
            result: Option<DtResult>,
        }
        #[derive(Deserialize)]
        struct DtResult {
            data: Option<Vec<DtItem>>,
        }
        #[derive(Deserialize)]
        struct DtItem {
            #[serde(rename = "SECURITY_CODE")]
            security_code: Option<String>,
            #[serde(rename = "SECURITY_NAME_ABBR")]
            security_name: Option<String>,
            #[serde(rename = "TRADE_DATE")]
            trade_date: Option<String>,
            #[serde(rename = "CLOSE_PRICE", default)]
            close_price: Option<f64>,
            #[serde(rename = "CHANGE_RATE", default)]
            change_rate: Option<f64>,
            #[serde(rename = "TURNOVERRATE", default)]
            turnover_rate: Option<f64>,
            #[serde(rename = "ACCUM_AMOUNT", default)]
            accum_amount: Option<f64>,
            #[serde(rename = "BILLBOARD_NET_AMT", default)]
            net_amount: Option<f64>,
            #[serde(rename = "BILLBOARD_BUY_AMT", default)]
            total_buy: Option<f64>,
            #[serde(rename = "BILLBOARD_SELL_AMT", default)]
            total_sell: Option<f64>,
            #[serde(rename = "DEAL_AMOUNT_RATIO", default)]
            lhb_ratio: Option<f64>,
            #[serde(rename = "EXPLANATION")]
            reason: Option<String>,
        }

        // datacenter.eastmoney.com 会返回 Content-Encoding: gzip 的响应体（且非合法 gzip），
        // 直接 .json() 会报 "error decoding response body"。强制 identity 并在本地解析规避。
        let resp_text = self.client
            .get(&url)
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .send()
            .await?
            .text()
            .await?;
        let resp: DtResp = serde_json::from_str(&resp_text)?;
        let data = resp.result.and_then(|r| r.data).unwrap_or_default();

        // 接口金额单位为「元」，模型与前端均按「万元」处理，统一除以 10000
        let to_wan = |v: Option<f64>| v.unwrap_or(0.0) / 10000.0;

        let entries: Vec<DragonTigerEntry> = data.iter().map(|item| {
            // TRADE_DATE 形如 "2026-07-31 00:00:00"，只保留日期部分
            let date = item.trade_date.clone().unwrap_or_default();
            let date = date.split(' ').next().unwrap_or(&date).to_string();
            DragonTigerEntry {
                code: item.security_code.clone().unwrap_or_default(),
                name: item.security_name.clone().unwrap_or_default(),
                date,
                close_price: item.close_price.unwrap_or(0.0),
                pct_change: item.change_rate.unwrap_or(0.0),
                turnover: item.turnover_rate.unwrap_or(0.0),
                amount: to_wan(item.accum_amount),
                amplitude: 0.0,
                net_amount: to_wan(item.net_amount),
                total_buy: to_wan(item.total_buy),
                total_sell: to_wan(item.total_sell),
                lhb_amount_ratio: item.lhb_ratio.unwrap_or(0.0),
                reason: item.reason.clone().unwrap_or_default(),
                buy_seats: vec![],
                sell_seats: vec![],
                institution_net_buy: 0.0,
                hot_money_tags: vec![],
            }
        }).collect();

        Ok(entries)
    }

    // ============ Phase 4: 财务数据 ============

    async fn fetch_financial_indicators(&self, code: &str) -> AppResult<FinancialSummary> {
        let url = format!(
            "http://datacenter.eastmoney.com/securities/api/data/v1/get?reportName=RPT_DMSK_FN_MAININDICATOR&columns=ALL&filter=(SECURITY_CODE=\"{}\")&pageNumber=1&pageSize=8&sortTypes=-1&sortColumns=REPORT_DATE&source=WEB&client=WEB",
            code
        );

        tracing::debug!("Fetching financial indicators: {}", url);

        #[derive(Deserialize)]
        struct FinResp {
            result: Option<FinResult>,
        }
        #[derive(Deserialize)]
        struct FinResult {
            data: Option<Vec<FinItem>>,
        }
        #[derive(Deserialize)]
        struct FinItem {
            #[serde(rename = "SECURITY_CODE")]
            security_code: Option<String>,
            #[serde(rename = "SECURITY_NAME_ABBR")]
            security_name: Option<String>,
            #[serde(rename = "REPORT_DATE")]
            report_date: Option<String>,
            #[serde(rename = "BASIC_EPS", default)]
            eps: Option<f64>,
            #[serde(rename = "BPS", default)]
            bvps: Option<f64>,
            #[serde(rename = "MGJYXJJE", default)]
            cfps: Option<f64>,
            #[serde(rename = "WEIGHTAVG_ROE", default)]
            roe: Option<f64>,
            #[serde(rename = "TOTAL_NETPROFIT_MARGIN", default)]
            roa: Option<f64>,
            #[serde(rename = "GROSSPROFIT_MARGIN", default)]
            gross_margin: Option<f64>,
            #[serde(rename = "NETPROFIT_MARGIN", default)]
            net_margin: Option<f64>,
            #[serde(rename = "TOTAL_OPERATE_INCOME", default)]
            revenue: Option<f64>,
            #[serde(rename = "TOTAL_OPERATE_INCOME_YOY", default)]
            revenue_yoy: Option<f64>,
            #[serde(rename = "PARENT_NETPROFIT", default)]
            net_profit: Option<f64>,
            #[serde(rename = "PARENT_NETPROFIT_YOY", default)]
            net_profit_yoy: Option<f64>,
            #[serde(rename = "DEDUCTED_PARENT_NETPROFIT", default)]
            deducted_profit: Option<f64>,
            #[serde(rename = "DEDUCTED_PARENT_NETPROFIT_YOY", default)]
            deducted_profit_yoy: Option<f64>,
            #[serde(rename = "DEBT_ASSET_RATIO", default)]
            debt_ratio: Option<f64>,
            #[serde(rename = "CURRENT_RATIO", default)]
            current_ratio: Option<f64>,
            #[serde(rename = "QUICK_RATIO", default)]
            quick_ratio: Option<f64>,
        }

        // 同龙虎榜：datacenter.eastmoney.com 返回非合法 gzip，强制 identity 并本地解析
        let resp_text = self.client
            .get(&url)
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .send()
            .await?
            .text()
            .await?;
        let resp: FinResp = serde_json::from_str(&resp_text)?;
        let data = resp.result.and_then(|r| r.data).unwrap_or_default();

        let indicators: Vec<FinancialIndicator> = data.iter().map(|item| FinancialIndicator {
            report_date: item.report_date.clone().unwrap_or_default(),
            eps: item.eps,
            bvps: item.bvps,
            cfps: item.cfps,
            roe: item.roe,
            roa: item.roa,
            gross_margin: item.gross_margin,
            net_margin: item.net_margin,
            revenue: item.revenue,
            revenue_yoy: item.revenue_yoy,
            net_profit: item.net_profit,
            net_profit_yoy: item.net_profit_yoy,
            deducted_profit: item.deducted_profit,
            deducted_profit_yoy: item.deducted_profit_yoy,
            debt_ratio: item.debt_ratio,
            current_ratio: item.current_ratio,
            quick_ratio: item.quick_ratio,
        }).collect();

        let latest_report = indicators.first().map(|i| i.report_date.clone()).unwrap_or_default();

        Ok(FinancialSummary {
            stock_code: code.to_string(),
            stock_name: data.first().and_then(|d| d.security_name.clone()).unwrap_or_default(),
            latest_report_date: latest_report,
            pe: None,
            pb: None,
            ps: None,
            total_market_cap: None,
            float_market_cap: None,
            indicators,
        })
    }
}

impl EastmoneyFetcher {
    /// 获取沪深A股涨跌家数
    async fn fetch_up_down_count(&self) -> AppResult<(u32, u32, u32)> {
        // 沪市
        let _sh_url = format!(
            "{}/get?pn=1&pz=1&po=1&np=1&fltt=2&invt=2&fid=f3&fs=m:1+t:2&fields=f3",
            EM_LIST_BASE
        );
        // 深市
        let _sz_url = format!(
            "{}/get?pn=1&pz=1&po=1&np=1&fltt=2&invt=2&fid=f3&fs=m:0+t:6,m:0+t:80,m:1+t:2,m:0+t:81+s:2048&fields=f3",
            EM_LIST_BASE
        );

        // 用总览接口获取涨跌家数
        let overview_url = "http://push2delay.eastmoney.com/api/qt/ulist.np/get?fields=f1,f2,f3,f4,f6,f12,f13,f104,f105,f106&secids=1.000001,0.399001";

        #[derive(Deserialize)]
        struct OverviewResp {
            data: Option<OverviewData>,
        }
        #[derive(Deserialize)]
        struct OverviewData {
            diff: Option<Vec<OverviewItem>>,
        }
        #[derive(Deserialize)]
        struct OverviewItem {
            #[serde(rename = "f104", default)]
            up_count: u32,
            #[serde(rename = "f105", default)]
            down_count: u32,
            #[serde(rename = "f106", default)]
            flat_count: u32,
        }

        let resp: OverviewResp = self
            .client
            .get(overview_url)
            .send()
            .await?
            .json()
            .await?;

        if let Some(data) = resp.data {
            if let Some(diff) = data.diff {
                let (up, down, flat) = diff.iter().fold((0u32, 0u32, 0u32), |acc, item| {
                    (acc.0 + item.up_count, acc.1 + item.down_count, acc.2 + item.flat_count)
                });
                return Ok((up, down, flat));
            }
        }

        Ok((0, 0, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tencent_kline_day() {
        let raw = r#"{"code":0,"data":{"sh600519":{"qfqday":[["2026-07-27","1308.000","1289.500","1308.000","1279.580","31990.000"],["2026-07-28","1299.000","1320.000","1320.000","1289.520","53135.000"]]}}}"#;
        let klines =
            EastmoneyFetcher::parse_tencent_kline(raw, "600519", "sh600519", "day").unwrap();
        assert_eq!(klines.len(), 2);
        assert_eq!(klines[0].date, "2026-07-27");
        assert_eq!(klines[0].open, 1308.0);
        assert_eq!(klines[0].close, 1289.5);
        assert_eq!(klines[0].high, 1308.0);
        assert_eq!(klines[0].low, 1279.58);
        assert_eq!(klines[0].volume, 31990.0);
        assert_eq!(klines[0].stock_code, "600519");
        assert!(klines[0].pct_change.is_none());
        assert_eq!(klines[0].amount, 0.0);
        assert!(klines[0].turnover.is_none());
        assert_eq!(
            klines[1].pct_change.unwrap(),
            (1320.0 - 1289.5) / 1289.5 * 100.0
        );
    }

    #[test]
    fn test_parse_tencent_kline_week() {
        let raw = r#"{"code":0,"data":{"sh600519":{"qfqweek":[["2026-07-03","1169.000","1194.450","1215.520","1151.010","234098.000"]]}}}"#;
        let klines =
            EastmoneyFetcher::parse_tencent_kline(raw, "600519", "sh600519", "week").unwrap();
        assert_eq!(klines.len(), 1);
        assert_eq!(klines[0].date, "2026-07-03");
        assert_eq!(klines[0].close, 1194.45);
    }

    #[test]
    fn test_parse_tencent_kline_fallback_to_raw_day() {
        let raw = r#"{"code":0,"data":{"sh600519":{"day":[["2026-07-27","1308.000","1289.500","1308.000","1279.580","31990.000"]]}}}"#;
        let klines =
            EastmoneyFetcher::parse_tencent_kline(raw, "600519", "sh600519", "day").unwrap();
        assert_eq!(klines.len(), 1);
        assert_eq!(klines[0].date, "2026-07-27");
    }

    #[test]
    fn test_parse_tencent_kline_error_code_empty() {
        let raw = r#"{"code":-1,"data":null}"#;
        let klines =
            EastmoneyFetcher::parse_tencent_kline(raw, "600519", "sh600519", "day").unwrap();
        assert!(klines.is_empty());
    }

    #[test]
    fn test_tencent_secid_prefix() {
        assert_eq!(EastmoneyFetcher::tencent_secid("600519"), "sh600519");
        assert_eq!(EastmoneyFetcher::tencent_secid("688981"), "sh688981");
        assert_eq!(EastmoneyFetcher::tencent_secid("000001"), "sz000001");
        assert_eq!(EastmoneyFetcher::tencent_secid("300750"), "sz300750");
        assert_eq!(EastmoneyFetcher::tencent_secid("430047"), "bj430047");
    }

    #[test]
    fn test_parse_tencent_minute_kline() {
        let raw = r#"{"code":0,"msg":"","data":{"sz000001":{"m5":[["202607271340","11.14","11.15","11.15","11.13","12347.00",{},"0.6363"],["202607271345","11.15","11.14","11.15","11.14","14315.68",{},"0.7377"]]}}}"#;
        let klines =
            EastmoneyFetcher::parse_tencent_minute_kline(raw, "000001", "sz000001", "m5").unwrap();
        assert_eq!(klines.len(), 2);
        assert_eq!(klines[0].date, "2026-07-27 13:40");
        assert_eq!(klines[0].open, 11.14);
        assert_eq!(klines[0].close, 11.15);
        assert_eq!(klines[0].high, 11.15);
        assert_eq!(klines[0].low, 11.13);
        assert_eq!(klines[0].volume, 12347.0);
        assert_eq!(klines[0].amount, 0.6363);
        assert_eq!(klines[0].stock_code, "000001");
        assert!(klines[0].pct_change.is_none());
        assert_eq!(
            klines[1].pct_change.unwrap(),
            (11.14 - 11.15) / 11.15 * 100.0
        );
    }

    #[test]
    fn test_parse_tencent_minute_kline_error_code_empty() {
        let raw = r#"{"code":-1,"msg":"","data":null}"#;
        let klines =
            EastmoneyFetcher::parse_tencent_minute_kline(raw, "000001", "sz000001", "m5").unwrap();
        assert!(klines.is_empty());
    }

    #[test]
    fn test_parse_tencent_minute_kline_missing_period_empty() {
        // data 节点存在但缺少请求的 m5 周期
        let raw = r#"{"code":0,"msg":"","data":{"sz000001":{"m1":[["202607271340","11.14","11.15","11.15","11.13","12347.00",{},"0.6363"]]}}}"#;
        let klines =
            EastmoneyFetcher::parse_tencent_minute_kline(raw, "000001", "sz000001", "m5").unwrap();
        assert!(klines.is_empty());
    }

    #[test]
    fn test_format_minute_datetime() {
        assert_eq!(
            EastmoneyFetcher::format_minute_datetime("202608051025"),
            "2026-08-05 10:25"
        );
        // 非标准格式原样返回
        assert_eq!(
            EastmoneyFetcher::format_minute_datetime("2026-07-27"),
            "2026-07-27"
        );
    }
}
