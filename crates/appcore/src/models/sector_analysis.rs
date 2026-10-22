use serde::{Deserialize, Serialize};

/// 单个上涨驱动因子（用于前端按贡献度展示）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasonFactor {
    /// 类别：技术面 / 基本面 / 资金面 / 催化剂
    pub category: String,
    /// 因子名（如 涨幅强度 / 主力净流入 / 业绩增速 / 龙虎榜）
    pub name: String,
    /// 贡献度 0..1
    pub weight: f64,
    /// 明细说明
    pub detail: String,
}

/// 板块上涨原因（技术面 + 基本面 + 催化剂）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectorUpReason {
    /// 上涨逻辑定性：业绩驱动 / 业绩+资金驱动 / 资金驱动 / 题材炒作 / 普涨跟随
    pub logic: String,
    /// 技术面原因（文字）
    pub technical: String,
    /// 基本面原因（文字）
    pub fundamental: String,
    /// 催化剂（文字）
    pub catalyst: String,
    /// 各维度驱动因子
    pub factors: Vec<ReasonFactor>,
}

/// 领涨股基本面快照（用于基本面验证）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectorLeaderFundamental {
    pub code: String,
    pub name: String,
    pub pct_change: Option<f64>,
    /// 营收同比 (%)
    pub revenue_yoy: Option<f64>,
    /// 归母净利润同比 (%)
    pub net_profit_yoy: Option<f64>,
    /// ROE (%)
    pub roe: Option<f64>,
    /// 毛利率 (%)
    pub gross_margin: Option<f64>,
    /// 净利率 (%)
    pub net_margin: Option<f64>,
    /// 最新报告期
    pub report_date: String,
}

/// 单个热点板块分析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectorHotspotItem {
    /// 热度排名（1 起）
    pub rank: usize,
    pub code: String,
    pub name: String,
    /// 板块类型：行业 / 概念
    pub sector_type: String,
    /// 板块涨幅 (%)
    pub change_pct: f64,
    /// 领涨股**名称**（如「芒果超媒」），仅用于展示
    pub leader_stock: Option<String>,
    /// 领涨股**代码**（如「300413」）
    ///
    /// 基本面查询与龙虎榜匹配都以它为准。上游东财的 f128 是名称、f140 才是代码，
    /// 两者必须分开存，否则会把名称当代码去查，静默查空。
    pub leader_code: Option<String>,
    /// 领涨股涨幅 (%)
    pub leader_pct: Option<f64>,
    /// 板块内上涨家数
    pub up_count: u32,
    /// 板块内下跌家数
    pub down_count: u32,
    /// 主力净流入（元）
    pub main_net_inflow: f64,
    /// 主力净流入占比 (%)
    pub main_net_ratio: f64,
    /// 综合热度评分 0..100
    pub strength_score: f64,
    /// 领涨股基本面（若有）
    pub leader_fundamental: Option<SectorLeaderFundamental>,
    /// 领涨股是否登上龙虎榜
    pub dragon_tiger_hit: bool,
    /// 上涨原因
    pub reason: SectorUpReason,
    /// 一句话摘要
    pub summary: String,
}

/// 市场热点板块整体分析
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectorHotspotAnalysis {
    /// 生成时间戳（秒）
    pub generated_at: i64,
    /// 上榜行业板块数
    pub industry_count: usize,
    /// 上榜概念板块数
    pub concept_count: usize,
    /// 热点板块列表（按热度降序）
    pub hot_sectors: Vec<SectorHotspotItem>,
    /// 整体结论
    pub conclusion: String,
}
