use appcore::models::*;
use appcore::KlinePeriod;
use fetcher::DataFetcher;
use indicators::{calc_macd, calc_kdj, calc_rsi, calc_boll, calc_support_resistance};
use std::sync::Arc;

/// AI 分析所需的全部数据上下文
pub struct AnalysisContext {
    pub quote: Quote,
    pub klines: Vec<Kline>,
    pub macd: Vec<indicators::MacdResult>,
    pub kdj: Vec<indicators::KdjResult>,
    pub rsi: Vec<indicators::RsiResult>,
    pub boll: Vec<indicators::BollResult>,
    pub support_resistance: indicators::SupportResistance,
    pub market: Option<MarketOverview>,
    pub sectors: Option<Vec<SectorRank>>,
}

/// 数据采集器
pub struct DataCollector {
    fetcher: Arc<dyn DataFetcher>,
}

impl DataCollector {
    pub fn new(fetcher: Arc<dyn DataFetcher>) -> Self {
        Self { fetcher }
    }

    /// 获取 fetcher 引用
    pub fn fetcher(&self) -> &Arc<dyn DataFetcher> {
        &self.fetcher
    }

    /// 采集个股分析所需的全量数据
    pub async fn collect_for_stock(&self, code: &str) -> anyhow::Result<AnalysisContext> {
        // 并行获取行情、K线、市场数据
        let (quote, klines, market, sectors) = tokio::join!(
            self.fetcher.fetch_quote(code),
            self.fetcher.fetch_kline(code, &KlinePeriod::Daily, None, None, Some(120)),
            self.fetch_market_overview_safe(),
            self.fetch_sectors_safe("industry"),
        );

        let quote = quote?;
        let klines = klines?;

        // 计算技术指标
        let closes: Vec<f64> = klines.iter().map(|k| k.close).collect();
        let dates: Vec<String> = klines.iter().map(|k| k.date.clone()).collect();
        let highs: Vec<f64> = klines.iter().map(|k| k.high).collect();
        let lows: Vec<f64> = klines.iter().map(|k| k.low).collect();

        let macd = calc_macd(&closes, &dates, 12, 26, 9);
        let kdj = calc_kdj(&highs, &lows, &closes, &dates, 9);
        let rsi = calc_rsi(&closes, &dates);
        let boll = calc_boll(&closes, &dates, 20, 2.0);
        let support_resistance = calc_support_resistance(&klines, 60);

        Ok(AnalysisContext {
            quote,
            klines,
            macd,
            kdj,
            rsi,
            boll,
            support_resistance,
            market,
            sectors,
        })
    }

    /// 采集市场分析所需数据
    pub async fn collect_for_market(&self) -> anyhow::Result<(MarketOverview, Vec<SectorRank>)> {
        let market = self.fetcher.fetch_market_overview().await?;
        let sectors = self.fetch_sectors_safe("industry").await.unwrap_or_default();
        Ok((market, sectors))
    }

    /// 采集多周期数据
    pub async fn collect_multi_period(&self, code: &str) -> anyhow::Result<String> {
        let (weekly, monthly) = tokio::join!(
            self.fetcher.fetch_kline(code, &KlinePeriod::Weekly, None, None, Some(26)),
            self.fetcher.fetch_kline(code, &KlinePeriod::Monthly, None, None, Some(12)),
        );

        let mut summary = String::new();

        if let Ok(wk) = weekly {
            if let Some(last) = wk.last() {
                summary.push_str(&format!(
                    "周线: 最新收盘 {:.2}, 本周涨跌 {:.2}%, 近26周最高 {:.2}, 最低 {:.2}\n",
                    last.close,
                    last.pct_change.unwrap_or(0.0),
                    wk.iter().map(|k| k.high).fold(f64::MIN, f64::max),
                    wk.iter().map(|k| k.low).fold(f64::MAX, f64::min),
                ));
            }
        }

        if let Ok(mo) = monthly {
            if let Some(last) = mo.last() {
                summary.push_str(&format!(
                    "月线: 最新收盘 {:.2}, 本月涨跌 {:.2}%, 近12月最高 {:.2}, 最低 {:.2}",
                    last.close,
                    last.pct_change.unwrap_or(0.0),
                    mo.iter().map(|k| k.high).fold(f64::MIN, f64::max),
                    mo.iter().map(|k| k.low).fold(f64::MAX, f64::min),
                ));
            }
        }

        if summary.is_empty() {
            summary = "多周期数据获取失败".to_string();
        }

        Ok(summary)
    }

    async fn fetch_market_overview_safe(&self) -> Option<MarketOverview> {
        self.fetcher.fetch_market_overview().await.ok()
    }

    async fn fetch_sectors_safe(&self, sector_type: &str) -> Option<Vec<SectorRank>> {
        self.fetcher.fetch_sector_ranks(sector_type).await.ok()
    }
}

/// 将数据上下文格式化为 Prompt 可用的字符串
pub fn format_quote_json(q: &Quote) -> String {
    serde_json::to_string_pretty(q).unwrap_or_default()
}

pub fn format_kline_summary(klines: &[Kline]) -> String {
    if klines.is_empty() {
        return "无K线数据".to_string();
    }

    let n = klines.len();
    let recent = &klines[n.saturating_sub(60)..];

    let mut lines = Vec::new();

    // 整体统计
    let highest = recent.iter().map(|k| k.high).fold(f64::MIN, f64::max);
    let lowest = recent.iter().map(|k| k.low).fold(f64::MAX, f64::min);
    let avg_vol = recent.iter().map(|k| k.volume).sum::<f64>() / recent.len() as f64;
    let avg_amount = recent.iter().map(|k| k.amount).sum::<f64>() / recent.len() as f64;

    lines.push(format!("近{}日统计: 最高 {:.2}, 最低 {:.2}, 日均成交量 {:.0}, 日均成交额 {:.0}",
        recent.len(), highest, lowest, avg_vol, avg_amount));

    // 近10日明细
    let detail = &klines[n.saturating_sub(10)..];
    lines.push("近10日K线:".to_string());
    for k in detail {
        lines.push(format!(
            "  {} | 开:{:.2} 收:{:.2} 高:{:.2} 低:{:.2} 涨跌:{:.2}% 成交量:{:.0} 成交额:{:.0} 换手:{:.2}%",
            k.date, k.open, k.close, k.high, k.low,
            k.pct_change.unwrap_or(0.0), k.volume, k.amount, k.turnover.unwrap_or(0.0)
        ));
    }

    lines.join("\n")
}

pub fn format_indicators_json(
    macd: &[indicators::MacdResult],
    kdj: &[indicators::KdjResult],
    rsi: &[indicators::RsiResult],
    boll: &[indicators::BollResult],
    current_price: f64,
) -> String {
    let last_macd = macd.last();
    let last_kdj = kdj.last();
    let last_rsi = rsi.last();
    let last_boll = boll.last();

    let mut parts = Vec::new();

    if let Some(m) = last_macd {
        let signal = if m.macd > 0.0 { "金叉/多头" } else { "死叉/空头" };
        parts.push(format!(
            "MACD: DIF={:.4}, DEA={:.4}, MACD柱={:.4} ({})",
            m.dif, m.dea, m.macd, signal
        ));
    }

    if let Some(k) = last_kdj {
        let signal = if k.k > k.d { "金叉" } else { "死叉" };
        let zone = if k.k > 80.0 {
            "超买区"
        } else if k.k < 20.0 {
            "超卖区"
        } else {
            "中性区"
        };
        parts.push(format!(
            "KDJ: K={:.2}, D={:.2}, J={:.2} ({}, {})",
            k.k, k.d, k.j, signal, zone
        ));
    }

    if let Some(r) = last_rsi {
        let zone = if r.rsi6 > 80.0 {
            "超买"
        } else if r.rsi6 < 20.0 {
            "超卖"
        } else if r.rsi6 > 50.0 {
            "偏强"
        } else {
            "偏弱"
        };
        parts.push(format!(
            "RSI: RSI6={:.2}, RSI12={:.2}, RSI24={:.2} ({})",
            r.rsi6, r.rsi12, r.rsi24, zone
        ));
    }

    if let Some(b) = last_boll {
        let position = if current_price > b.upper {
            "突破上轨"
        } else if current_price < b.lower {
            "跌破下轨"
        } else if current_price > b.mid {
            "中轨上方"
        } else {
            "中轨下方"
        };
        parts.push(format!(
            "布林带: 上轨={:.2}, 中轨={:.2}, 下轨={:.2}, 当前价={:.2} ({})",
            b.upper, b.mid, b.lower, current_price, position
        ));
    }

    if parts.is_empty() {
        "技术指标数据不足".to_string()
    } else {
        parts.join("\n")
    }
}

pub fn format_sr_json(sr: &indicators::SupportResistance) -> String {
    let mut parts = Vec::new();
    parts.push(format!("当前价格: {:.2}", sr.current_price));

    if !sr.resistances.is_empty() {
        parts.push("压力位:".to_string());
        for r in &sr.resistances {
            let strength = match r.strength {
                indicators::PriceStrength::Strong => "强",
                indicators::PriceStrength::Medium => "中",
                indicators::PriceStrength::Weak => "弱",
            };
            parts.push(format!("  {:.2} ({}) - {}", r.price, strength, r.source));
        }
    }

    if !sr.supports.is_empty() {
        parts.push("支撑位:".to_string());
        for s in &sr.supports {
            let strength = match s.strength {
                indicators::PriceStrength::Strong => "强",
                indicators::PriceStrength::Medium => "中",
                indicators::PriceStrength::Weak => "弱",
            };
            parts.push(format!("  {:.2} ({}) - {}", s.price, strength, s.source));
        }
    }

    parts.join("\n")
}

pub fn format_market_json(market: &MarketOverview) -> String {
    let mut parts = Vec::new();

    for idx in &market.indices {
        parts.push(format!(
            "{}: {:.2} ({:+.2}%, {:+.2})",
            idx.name, idx.price, idx.pct_change, idx.change
        ));
    }

    parts.push(format!(
        "涨家数: {}, 跌家数: {}, 平家数: {}",
        market.up_count, market.down_count, market.flat_count
    ));
    parts.push(format!(
        "涨停: {}, 跌停: {}, 总成交额: {:.0}亿",
        market.limit_up_count, market.limit_down_count, market.total_amount / 1e8
    ));

    parts.join("\n")
}

pub fn format_sectors_json(sectors: &[SectorRank]) -> String {
    if sectors.is_empty() {
        return "无板块数据".to_string();
    }

    let mut sorted = sectors.to_vec();
    sorted.sort_by(|a, b| b.change_pct.partial_cmp(&a.change_pct).unwrap_or(std::cmp::Ordering::Equal));

    let top = &sorted[..sorted.len().min(10)];
    let bottom = if sorted.len() > 10 {
        &sorted[sorted.len().saturating_sub(5)..]
    } else {
        &[]
    };

    let mut parts = Vec::new();
    parts.push("涨幅前10板块:".to_string());
    for s in top {
        parts.push(format!(
            "  {} ({:+.2}%) - 领涨股: {} ({:+.2}%)",
            s.name, s.change_pct,
            s.leader_stock.as_deref().unwrap_or("N/A"),
            s.leader_pct.unwrap_or(0.0)
        ));
    }

    if !bottom.is_empty() {
        parts.push("跌幅前5板块:".to_string());
        for s in bottom {
            parts.push(format!(
                "  {} ({:+.2}%)",
                s.name, s.change_pct
            ));
        }
    }

    parts.join("\n")
}
