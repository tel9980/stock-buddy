use serde::{Deserialize, Serialize};

/// 自选股条目（含持仓成本价与持股数，用于做T联动）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchlistItem {
    pub stock_code: String,
    /// 持仓成本价（元），未设置时为 0
    pub cost_price: f64,
    /// 持股数量（股），未设置时为 0
    pub shares: i64,
}
