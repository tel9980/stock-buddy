use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// K 线周期
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KlinePeriod {
    Daily,
    Weekly,
    Monthly,
    Minute1,
    Minute5,
    Minute15,
    Minute30,
    Minute60,
}

impl KlinePeriod {
    /// 东方财富 API 的 klt 参数
    pub fn em_klt(&self) -> &str {
        match self {
            KlinePeriod::Daily => "101",
            KlinePeriod::Weekly => "102",
            KlinePeriod::Monthly => "103",
            KlinePeriod::Minute1 => "1",
            KlinePeriod::Minute5 => "5",
            KlinePeriod::Minute15 => "15",
            KlinePeriod::Minute30 => "30",
            KlinePeriod::Minute60 => "60",
        }
    }
}

impl FromStr for KlinePeriod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "daily" | "D" | "d" => Ok(KlinePeriod::Daily),
            "weekly" | "W" | "w" => Ok(KlinePeriod::Weekly),
            "monthly" | "M" => Ok(KlinePeriod::Monthly),
            "1" => Ok(KlinePeriod::Minute1),
            "5" => Ok(KlinePeriod::Minute5),
            "15" => Ok(KlinePeriod::Minute15),
            "30" => Ok(KlinePeriod::Minute30),
            "60" => Ok(KlinePeriod::Minute60),
            other => Err(format!("Unknown period: {}", other)),
        }
    }
}

/// K 线数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kline {
    pub stock_code: String,
    pub date: String,
    pub open: f64,
    pub close: f64,
    pub high: f64,
    pub low: f64,
    pub volume: f64,
    pub amount: f64,
    pub pct_change: Option<f64>,
    pub turnover: Option<f64>,
}

/// K 线请求参数
#[derive(Debug, Clone, Deserialize)]
pub struct KlineQuery {
    pub period: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub limit: Option<u32>,
}
