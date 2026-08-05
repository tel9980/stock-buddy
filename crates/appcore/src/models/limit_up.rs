use serde::{Deserialize, Serialize};

/// 涨停股信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitUpStock {
    pub code: String,
    pub name: String,
    /// 最新价
    pub price: f64,
    /// 涨跌幅 (%)
    pub pct_change: f64,
    /// 连续涨停天数
    pub consecutive_days: u32,
    /// 首次涨停时间 (HH:MM:SS)
    pub first_limit_time: Option<String>,
    /// 最后涨停时间
    pub last_limit_time: Option<String>,
    /// 炸板次数
    pub break_count: u32,
    /// 封单量（手）
    pub seal_volume: f64,
    /// 封单金额（万元）
    pub seal_amount: f64,
    /// 封板强度 (封单金额/成交额)
    pub seal_strength: Option<f64>,
    /// 成交额（万元）
    pub amount: f64,
    /// 换手率 (%)
    pub turnover: f64,
    /// 流通市值（万元）
    pub float_market_cap: f64,
    /// 所属行业
    pub industry: Option<String>,
    /// 涨停原因/题材
    pub reason: Option<String>,
    /// 振幅 (%)
    pub amplitude: f64,
}

/// 跌停股信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitDownStock {
    pub code: String,
    pub name: String,
    pub price: f64,
    pub pct_change: f64,
    /// 连续跌停天数
    pub consecutive_days: u32,
    /// 封单金额（万元）
    pub seal_amount: f64,
    /// 成交额（万元）
    pub amount: f64,
    /// 换手率 (%)
    pub turnover: f64,
    /// 所属行业
    pub industry: Option<String>,
}

/// 炸板股信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokenBoardStock {
    pub code: String,
    pub name: String,
    pub price: f64,
    pub pct_change: f64,
    /// 最高涨幅 (%)
    pub max_pct: f64,
    /// 炸板次数
    pub break_count: u32,
    /// 成交额（万元）
    pub amount: f64,
    /// 换手率 (%)
    pub turnover: f64,
    /// 所属行业
    pub industry: Option<String>,
}

/// 连板梯队
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsecutiveBoardTier {
    /// 连板天数
    pub days: u32,
    /// 该梯队的股票数量
    pub count: u32,
    /// 该梯队的股票列表
    pub stocks: Vec<LimitUpStock>,
}

/// 涨停复盘全景
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitUpReview {
    /// 涨停股列表
    pub limit_ups: Vec<LimitUpStock>,
    /// 涨停总数
    pub limit_up_count: u32,
    /// 一字板数量
    pub one_word_count: u32,
    /// T字板数量
    pub t_shape_count: u32,
    /// 炸板股列表
    pub broken_boards: Vec<BrokenBoardStock>,
    /// 炸板数
    pub broken_count: u32,
    /// 跌停股列表
    pub limit_downs: Vec<LimitDownStock>,
    /// 跌停数
    pub limit_down_count: u32,
    /// 连板梯队
    pub consecutive_tiers: Vec<ConsecutiveBoardTier>,
    /// 最高连板
    pub max_consecutive: u32,
    /// 涨停题材/原因分布
    pub reason_distribution: Vec<(String, u32)>,
    /// 炸板率 (%)
    pub break_rate: f64,
    /// 更新时间
    pub timestamp: i64,
}
