use async_trait::async_trait;
use appcore::models::*;
use appcore::{AppError, AppResult};
use crate::traits::DataFetcher;
use reqwest::Client;
use std::time::{SystemTime, UNIX_EPOCH};

/// 新浪财经数据采集器
pub struct SinaFetcher {
    client: Client,
}

impl SinaFetcher {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)")
            .build()
            .expect("Failed to create HTTP client");
        Self { client }
    }

    fn sina_code(code: &str) -> String {
        let market = Market::from_code(code);
        let prefix = match market {
            Market::Sh => "sh",
            Market::Sz => "sz",
            Market::Bj => "bj",
        };
        format!("{}{}", prefix, code)
    }

    fn now_ts() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }
}

impl Default for SinaFetcher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataFetcher for SinaFetcher {
    async fn fetch_quote(&self, code: &str) -> AppResult<Quote> {
        let sina_code = Self::sina_code(code);
        let url = format!("http://hq.sinajs.cn/list={}", sina_code);

        let resp = self
            .client
            .get(&url)
            .header("Referer", "https://finance.sina.com.cn")
            .send()
            .await?;

        let text = resp.text().await?;
        let text = text.trim();

        // 格式: var hq_str_sh600000="浦发银行,7.12,7.10,..."
        let start = text
            .find('"')
            .ok_or_else(|| AppError::Parse("Invalid sina response".into()))?;
        let end = text.rfind('"').ok_or_else(|| {
            AppError::Parse("Invalid sina response".into())
        })?;
        let content = &text[start + 1..end];
        let parts: Vec<&str> = content.split(',').collect();

        if parts.len() < 10 {
            return Err(AppError::Parse(format!(
                "Unexpected sina response for {}: {}",
                code, content
            )));
        }

        let name = parts[0].to_string();
        let open: f64 = parts[1].parse().unwrap_or(0.0);
        let pre_close: f64 = parts[2].parse().unwrap_or(0.0);
        let price: f64 = parts[3].parse().unwrap_or(0.0);
        let high: f64 = parts[4].parse().unwrap_or(0.0);
        let low: f64 = parts[5].parse().unwrap_or(0.0);
        let volume: f64 = parts[8].parse().unwrap_or(0.0);
        let amount: f64 = parts[9].parse().unwrap_or(0.0);
        let change = price - pre_close;
        let pct_change = if pre_close > 0.0 {
            (change / pre_close) * 100.0
        } else {
            0.0
        };

        Ok(Quote {
            code: code.to_string(),
            name,
            price,
            pct_change,
            change,
            open,
            pre_close,
            high,
            low,
            volume,
            amount,
            turnover: None,
            bid_price: parts.get(10).and_then(|v| v.parse().ok()),
            ask_price: parts.get(20).and_then(|v| v.parse().ok()),
            timestamp: Self::now_ts(),
        })
    }

    async fn fetch_quotes(&self, codes: &[&str]) -> AppResult<Vec<Quote>> {
        let sina_codes: Vec<String> =
            codes.iter().map(|c| Self::sina_code(c)).collect();
        let url = format!("http://hq.sinajs.cn/list={}", sina_codes.join(","));

        let resp = self
            .client
            .get(&url)
            .header("Referer", "https://finance.sina.com.cn")
            .send()
            .await?;

        let text = resp.text().await?;
        let mut quotes = Vec::with_capacity(codes.len());

        for (i, code) in codes.iter().enumerate() {
            let var_name = format!("hq_str_{}=", sina_codes[i]);
            if let Some(start) = text[i..].find(&var_name) {
                let abs_start = i + start + var_name.len();
                if let Some(end) = text[abs_start..].find('"') {
                    let content = &text[abs_start..abs_start + end];
                    let parts: Vec<&str> = content.split(',').collect();
                    if parts.len() >= 10 {
                        let name = parts[0].to_string();
                        let pre_close: f64 = parts[2].parse().unwrap_or(0.0);
                        let price: f64 = parts[3].parse().unwrap_or(0.0);
                        let change = price - pre_close;
                        let pct_change = if pre_close > 0.0 {
                            (change / pre_close) * 100.0
                        } else {
                            0.0
                        };
                        quotes.push(Quote {
                            code: code.to_string(),
                            name,
                            price,
                            pct_change,
                            change,
                            open: parts[1].parse().unwrap_or(0.0),
                            pre_close,
                            high: parts[4].parse().unwrap_or(0.0),
                            low: parts[5].parse().unwrap_or(0.0),
                            volume: parts[8].parse().unwrap_or(0.0),
                            amount: parts[9].parse().unwrap_or(0.0),
                            turnover: None,
                            bid_price: None,
                            ask_price: None,
                            timestamp: Self::now_ts(),
                        });
                    }
                }
            }
        }

        Ok(quotes)
    }

    async fn fetch_kline(
        &self,
        _code: &str,
        _period: &KlinePeriod,
        _start: Option<&str>,
        _end: Option<&str>,
        _limit: Option<u32>,
    ) -> AppResult<Vec<Kline>> {
        // 新浪 K 线接口较复杂, Phase 1 暂不支持, 使用东方财富
        Err(AppError::Internal(
            "Sina kline not implemented, use EastmoneyFetcher".into(),
        ))
    }

    async fn fetch_sector_ranks(&self, _sector_type: &str) -> AppResult<Vec<SectorRank>> {
        Err(AppError::Internal(
            "Sina sector ranks not implemented, use EastmoneyFetcher".into(),
        ))
    }

    async fn fetch_sector_stocks(&self, _sector_code: &str, _page: u32, _page_size: u32) -> AppResult<(Vec<SectorStock>, u32)> {
        Err(AppError::Internal(
            "Sina sector stocks not implemented, use EastmoneyFetcher".into(),
        ))
    }

    async fn fetch_market_overview(&self) -> AppResult<MarketOverview> {
        Err(AppError::Internal(
            "Sina market overview not implemented, use EastmoneyFetcher".into(),
        ))
    }

    // Phase 4 stubs
    async fn fetch_capital_flow(&self, _code: &str, _limit: Option<u32>) -> AppResult<Vec<CapitalFlow>> {
        Err(AppError::Internal("Sina capital flow not implemented".into()))
    }

    async fn fetch_sector_capital_flows(&self) -> AppResult<Vec<SectorCapitalFlow>> {
        Err(AppError::Internal("Sina sector capital flows not implemented".into()))
    }

    async fn fetch_north_bound_flow(&self, _limit: Option<u32>) -> AppResult<Vec<NorthBoundFlow>> {
        Err(AppError::Internal("Sina north bound flow not implemented".into()))
    }

    async fn fetch_north_bound_holdings(&self) -> AppResult<Vec<NorthBoundHoldings>> {
        Err(AppError::Internal("Sina north bound holdings not implemented".into()))
    }

    async fn fetch_market_capital_flow(&self) -> AppResult<MarketCapitalFlow> {
        Err(AppError::Internal("Sina market capital flow not implemented".into()))
    }

    async fn fetch_margin_trading(&self, _limit: Option<u32>) -> AppResult<Vec<MarginTrading>> {
        Err(AppError::Internal("Sina margin trading not implemented".into()))
    }

    async fn fetch_limit_ups(&self) -> AppResult<Vec<LimitUpStock>> {
        Err(AppError::Internal("Sina limit ups not implemented".into()))
    }

    async fn fetch_limit_downs(&self) -> AppResult<Vec<LimitDownStock>> {
        Err(AppError::Internal("Sina limit downs not implemented".into()))
    }

    async fn fetch_broken_boards(&self) -> AppResult<Vec<BrokenBoardStock>> {
        Err(AppError::Internal("Sina broken boards not implemented".into()))
    }

    async fn fetch_limit_up_review(&self) -> AppResult<LimitUpReview> {
        Err(AppError::Internal("Sina limit up review not implemented".into()))
    }

    async fn fetch_dragon_tiger(&self, _date: Option<&str>) -> AppResult<Vec<DragonTigerEntry>> {
        Err(AppError::Internal("Sina dragon tiger not implemented".into()))
    }

    async fn fetch_financial_indicators(&self, _code: &str) -> AppResult<FinancialSummary> {
        Err(AppError::Internal("Sina financial indicators not implemented".into()))
    }
}
