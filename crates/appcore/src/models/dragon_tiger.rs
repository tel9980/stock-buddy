use serde::{Deserialize, Serialize};

/// 龙虎榜营业部席位
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerSeat {
    /// 营业部名称
    pub name: String,
    /// 买入金额（万元）
    pub buy_amount: f64,
    /// 卖出金额（万元）
    pub sell_amount: f64,
    /// 净买入（万元）
    pub net_amount: f64,
}

/// 龙虎榜上榜条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DragonTigerEntry {
    pub code: String,
    pub name: String,
    /// 上榜日期
    pub date: String,
    /// 收盘价
    pub close_price: f64,
    /// 涨跌幅 (%)
    pub pct_change: f64,
    /// 换手率 (%)
    pub turnover: f64,
    /// 成交额（万元）
    pub amount: f64,
    /// 振幅 (%)
    pub amplitude: f64,
    /// 龙虎榜净买入（万元）
    pub net_amount: f64,
    /// 龙虎榜买入总额（万元）
    pub total_buy: f64,
    /// 龙虎榜卖出总额（万元）
    pub total_sell: f64,
    /// 龙虎榜成交额占比 (%)
    pub lhb_amount_ratio: f64,
    /// 上榜原因
    pub reason: String,
    /// 买入前5席位
    pub buy_seats: Vec<BrokerSeat>,
    /// 卖出前5席位
    pub sell_seats: Vec<BrokerSeat>,
    /// 机构席位净买入（万元）
    pub institution_net_buy: f64,
    /// 游资标识
    pub hot_money_tags: Vec<String>,
}

/// 龙虎榜统计概览
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DragonTigerOverview {
    /// 今日上榜总数
    pub total_count: u32,
    /// 净买入家数
    pub net_buy_count: u32,
    /// 净卖出家数
    pub net_sell_count: u32,
    /// 机构净买入总额（万元）
    pub institution_net_buy_total: f64,
    /// 上榜个股列表
    pub entries: Vec<DragonTigerEntry>,
    /// 活跃营业部排行
    pub active_seats: Vec<BrokerSeat>,
    /// 更新时间
    pub timestamp: i64,
}
