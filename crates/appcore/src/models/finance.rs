use serde::{Deserialize, Serialize};

/// 主要财务指标
///
/// 单位约定：金额类字段一律**万元**（上游东财返回的是元，抓取层负责 ÷10000）；
/// 每股类为元；比率类为百分数（16.75 表示 16.75%）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinancialIndicator {
    /// 报告期 (e.g. 2024-12-31)
    pub report_date: String,
    /// 报告期类型：年报 / 中报 / 一季报 / 三季报
    ///
    /// 东财只给到「期」不给「是否累计」的显式标记，但同一只股票里
    /// 一季报/中报/三季报/年报本身就是累计口径，呈现期别能让读表的人
    /// 一眼看出「中报的 35.57 元 EPS 是 6 个月累计」而非单季。
    pub report_type: Option<String>,
    /// 基本每股收益（元）
    pub eps: Option<f64>,
    /// 每股净资产（元）
    pub bvps: Option<f64>,
    /// 每股经营现金流（元）
    pub cfps: Option<f64>,
    /// 净资产收益率 ROE (%)
    pub roe: Option<f64>,
    /// 总资产净利率 ROA (%)
    pub roa: Option<f64>,
    /// 毛利率 (%)
    pub gross_margin: Option<f64>,
    /// 净利率 (%)
    pub net_margin: Option<f64>,
    /// 营业收入（万元）
    pub revenue: Option<f64>,
    /// 营业收入同比增长 (%)
    pub revenue_yoy: Option<f64>,
    /// 归母净利润（万元）
    pub net_profit: Option<f64>,
    /// 归母净利润同比增长 (%)
    pub net_profit_yoy: Option<f64>,
    /// 扣非净利润（万元）
    pub deducted_profit: Option<f64>,
    /// 扣非净利润同比增长 (%)
    pub deducted_profit_yoy: Option<f64>,
    /// 资产负债率 (%)
    pub debt_ratio: Option<f64>,
    /// 流动比率
    pub current_ratio: Option<f64>,
    /// 速动比率
    pub quick_ratio: Option<f64>,
}

/// 财务报表摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinancialSummary {
    pub stock_code: String,
    pub stock_name: String,
    /// 最新报告期
    pub latest_report_date: String,
    /// PE (市盈率)
    pub pe: Option<f64>,
    /// PB (市净率)
    pub pb: Option<f64>,
    /// PS (市销率)
    pub ps: Option<f64>,
    /// 总市值（万元）
    pub total_market_cap: Option<f64>,
    /// 流通市值（万元）
    pub float_market_cap: Option<f64>,
    /// 近4个报告期的主要财务指标
    pub indicators: Vec<FinancialIndicator>,
}
