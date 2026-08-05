use serde::{Deserialize, Serialize};

/// 个股资金流向日数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapitalFlow {
    pub stock_code: String,
    pub date: String,
    /// 主力净流入（万元）
    pub main_net_inflow: f64,
    /// 主力净流入占比 (%)
    pub main_net_ratio: f64,
    /// 超大单净流入（万元）
    pub huge_net_inflow: f64,
    /// 超大单净流入占比 (%)
    pub huge_net_ratio: f64,
    /// 大单净流入（万元）
    pub large_net_inflow: f64,
    /// 大单净流入占比 (%)
    pub large_net_ratio: f64,
    /// 中单净流入（万元）
    pub medium_net_inflow: f64,
    /// 中单净流入占比 (%)
    pub medium_net_ratio: f64,
    /// 小单净流入（万元）
    pub small_net_inflow: f64,
    /// 小单净流入占比 (%)
    pub small_net_ratio: f64,
    /// 收盘价
    pub close_price: f64,
    /// 涨跌幅 (%)
    pub pct_change: f64,
}

/// 行业/概念板块资金流向
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectorCapitalFlow {
    pub code: String,
    pub name: String,
    /// 主力净流入（万元）
    pub main_net_inflow: f64,
    /// 主力净流入占比 (%)
    pub main_net_ratio: f64,
    /// 超大单净流入（万元）
    pub huge_net_inflow: f64,
    /// 大单净流入（万元）
    pub large_net_inflow: f64,
    /// 中单净流入（万元）
    pub medium_net_inflow: f64,
    /// 小单净流入（万元）
    pub small_net_inflow: f64,
    /// 涨跌幅 (%)
    pub pct_change: f64,
    /// 上榜股票数
    pub stock_count: Option<u32>,
}

/// 北向资金流向
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NorthBoundFlow {
    pub date: String,
    /// 沪股通净流入（万元）
    pub sh_net_inflow: f64,
    /// 深股通净流入（万元）
    pub sz_net_inflow: f64,
    /// 合计净流入（万元）
    pub total_net_inflow: f64,
    /// 沪股通余额（万元）
    pub sh_balance: f64,
    /// 深股通余额（万元）
    pub sz_balance: f64,
}

/// 融资融券概览
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarginTrading {
    pub date: String,
    /// 融资余额（元）
    pub margin_balance: f64,
    /// 融券余额（元）
    pub short_balance: f64,
    /// 融资融券余额（元）
    pub total_balance: f64,
    /// 融资买入额（元）
    pub margin_buy: f64,
    /// 融资偿还额（元）
    pub margin_repay: f64,
    /// 融资净买入（元）
    pub margin_net_buy: f64,
    /// 融券卖出量（股）
    pub short_sell_vol: f64,
    /// 融券偿还量（股）
    pub short_repay_vol: f64,
    /// 融券净卖出（股）
    pub short_net_sell: f64,
}

/// 北向持股季度汇总（自 2024-08-19 起交易所仅按季度披露北向持仓）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NorthBoundHoldings {
    pub date: String,
    /// 持股市值（元）
    pub hold_market_cap: f64,
    /// 本期增减持市值（元）
    pub add_market_cap: f64,
    /// 增减持比例（%）
    pub change_rate: f64,
}

/// 沪深两市主力资金（实时，元）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketCapitalFlow {
    pub date: String,
    /// 主力净流入（元）
    pub main_net_inflow: f64,
    /// 超大单净流入（元）
    pub huge_net_inflow: f64,
    /// 大单净流入（元）
    pub large_net_inflow: f64,
    /// 中单净流入（元）
    pub medium_net_inflow: f64,
    /// 小单净流入（元）
    pub small_net_inflow: f64,
}

/// 市场资金流向总览
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapitalFlowOverview {
    /// 北向资金（每日净流入，已停止披露）
    pub north_bound: Option<NorthBoundFlow>,
    /// 北向持股（季度）
    pub north_bound_holdings: Option<NorthBoundHoldings>,
    /// 融资融券
    pub margin_trading: Option<MarginTrading>,
    /// 沪深两市主力资金（实时）
    pub market_capital_flow: Option<MarketCapitalFlow>,
    /// 行业资金流排行（Top N）
    pub sector_flows: Vec<SectorCapitalFlow>,
    /// 更新时间
    pub timestamp: i64,
}
