use serde::{Deserialize, Serialize};

/// 实时行情
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quote {
    pub code: String,
    pub name: String,
    pub price: f64,
    pub pct_change: f64,
    pub change: f64,
    pub open: f64,
    pub pre_close: f64,
    pub high: f64,
    pub low: f64,
    pub volume: f64,
    pub amount: f64,
    pub turnover: Option<f64>,
    pub bid_price: Option<f64>,
    pub ask_price: Option<f64>,
    pub timestamp: i64,
}

/// 市场指数行情
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexQuote {
    pub code: String,
    pub name: String,
    pub price: f64,
    pub pct_change: f64,
    pub change: f64,
    pub volume: f64,
    pub amount: f64,
    pub up_count: Option<u32>,
    pub down_count: Option<u32>,
    pub flat_count: Option<u32>,
}
