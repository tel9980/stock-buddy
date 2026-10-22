use serde::{Deserialize, Serialize};

/// 板块排行
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectorRank {
    pub code: String,
    pub name: String,
    pub sector_type: String,
    pub change_pct: f64,
    /// 领涨股**名称**（如「芒果超媒」）
    ///
    /// 只用于展示。凡是拿它去查财报、匹配龙虎榜的，一律改用 `leader_code`——
    /// 东财 f128 给的是名称，早年曾直接拿它当代码用，导致基本面查询恒为空、
    /// 龙虎榜永远匹配不上，两个功能静默失效了很久。
    pub leader_stock: Option<String>,
    /// 领涨股**代码**（如「300413」，东财 f140）
    pub leader_code: Option<String>,
    pub leader_pct: Option<f64>,
    pub net_amount: Option<f64>,
    pub up_count: Option<u32>,
    pub down_count: Option<u32>,
}

/// 板块成分股
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectorStock {
    pub code: String,
    pub name: String,
    pub price: f64,
    pub pct_change: f64,
    pub change: f64,
    pub volume: f64,
    pub amount: f64,
    pub high: f64,
    pub low: f64,
    pub open: f64,
    pub pre_close: f64,
    pub turnover: f64,
}

/// 大盘总览
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketOverview {
    pub indices: Vec<IndexSummary>,
    pub up_count: u32,
    pub down_count: u32,
    pub flat_count: u32,
    pub limit_up_count: u32,
    pub limit_down_count: u32,
    pub total_amount: f64,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexSummary {
    pub code: String,
    pub name: String,
    pub price: f64,
    pub pct_change: f64,
    pub change: f64,
    pub volume: f64,
    pub amount: f64,
}
