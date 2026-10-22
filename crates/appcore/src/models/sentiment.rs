use serde::{Deserialize, Serialize};

use crate::models::JobRunInfo;

/// 题材个股（同花顺强势股原始条目）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeStock {
    pub code: String,
    pub name: String,
    /// 涨跌幅 (%)
    pub pct_change: f64,
    /// 同花顺原始归因字符串，多个题材以「+」连接
    pub reason: String,
}

/// 题材热度统计项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotTheme {
    /// 题材标签名
    pub tag: String,
    /// 该题材下强势股只数
    pub count: u32,
    /// 题材内领涨股名称
    pub leader: Option<String>,
    /// 领涨股涨跌幅 (%)
    pub leader_pct: f64,
    /// 题材内个股平均涨跌幅 (%)
    pub avg_pct: f64,
    /// 题材内个股（按涨跌幅降序）
    pub stocks: Vec<ThemeStock>,
}

/// 题材热点汇总
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeSummary {
    /// 强势股只数
    pub stock_count: u32,
    /// 拆出的题材标签总个数（不去重）
    pub tag_total: u32,
    /// 去重后的题材个数
    pub theme_total: u32,
    /// 按热度降序的题材列表
    pub themes: Vec<HotTheme>,
    /// 最热题材（列表首项，便于前端直接取用）
    pub top_tag: Option<String>,
}

/// 上市原因统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasonStat {
    pub reason: String,
    pub count: u32,
}

/// 龙虎榜汇总
///
/// 注意口径：同一只股票可能触发多个上榜原因，东财返回多条记录且各记录的
/// `BILLBOARD_NET_AMT` 可能不同。因此同时提供「按个股去重」与「按记录求和」两套汇总。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LhbSummary {
    /// 去重后的上榜个股家数
    pub stock_count: u32,
    /// 原始记录条数
    pub record_count: u32,
    /// 按个股去重后的净买入合计（万元）
    pub net_total: f64,
    /// 原始记录直接求和的净买入（万元）
    pub records_sum: f64,
    /// 买入总额（去重口径，万元）
    pub buy_total: f64,
    /// 卖出总额（去重口径，万元）
    pub sell_total: f64,
    /// 净买入为正的个股家数
    pub net_buy_count: u32,
    /// 净买入为负的个股家数
    pub net_sell_count: u32,
    /// 上榜个股成交额合计（去重口径，万元）
    pub amount_total: f64,
    /// 明细记录（按净买入降序）
    pub entries: Vec<crate::DragonTigerEntry>,
    /// 上榜原因 TOP N
    pub top_reasons: Vec<ReasonStat>,
}

/// 行业板块表现
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndustryBoard {
    pub name: String,
    /// 涨跌幅 (%)
    pub change_pct: f64,
    /// 成分股上涨家数
    pub up_count: u32,
    /// 成分股下跌家数
    pub down_count: u32,
    /// 成分股平盘家数
    pub flat_count: u32,
    /// 领涨股名称
    pub leader: Option<String>,
    /// 领涨股代码
    pub leader_code: Option<String>,
    /// 领涨股涨跌幅 (%)
    pub leader_pct: f64,
    /// 板块成交额（万元）
    ///
    /// 历史交易日为 `None`：东财历史 K 线接口在本机网络不可达，
    /// 而可用的腾讯日线接口不含成交额字段，故不做估算、宁缺勿假。
    pub amount: Option<f64>,
    /// 东财官方披露的成分股家数（聚合口径下可能为 0）
    pub member_count: u32,
    /// 实际参与涨跌幅计算的个股家数
    pub calc_count: u32,
}

/// 指数收盘快照
///
/// 与 `quote::IndexQuote`（实时行情）区分：这里是「指定交易日的收盘 + 涨跌幅」，
/// 用于日报的指数条展示。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexDailyQuote {
    pub code: String,
    pub name: String,
    pub close: f64,
    /// 涨跌幅 (%)
    pub pct_change: f64,
}

/// 全市场宽度
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketBreadth {
    pub up_count: u32,
    pub down_count: u32,
    pub flat_count: u32,
    /// 纳入统计的个股家数
    pub covered_count: u32,
    /// 全市场成交额（万元）。历史交易日为 `None`，原因同 `IndustryBoard::amount`
    pub total_amount: Option<f64>,
    pub indices: Vec<IndexDailyQuote>,
}

/// 方法学与口径说明
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Methodology {
    /// 行业涨跌幅的计算方式
    pub board_method: String,
    /// 是否已使用缓存结果
    pub from_cache: bool,
    /// 计算耗时（毫秒）
    pub elapsed_ms: u64,
    /// 补充注意事项
    pub notes: Vec<String>,
}

/// 市场情绪日报（指定交易日）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentDaily {
    /// 交易日 YYYY-MM-DD
    pub date: String,
    /// 报告生成时间戳
    pub generated_at: i64,
    pub lhb: LhbSummary,
    pub themes: ThemeSummary,
    pub boards: Vec<IndustryBoard>,
    pub breadth: MarketBreadth,
    pub methodology: Methodology,
    /// 数据来源清单（用于页脚披露）
    pub sources: Vec<String>,
}

/// 日报查询参数（路由层使用）
#[derive(Debug, Clone, Deserialize)]
pub struct SentimentQuery {
    /// 交易日 YYYY-MM-DD 或 YYYYMMDD，缺省为最近交易日
    pub date: Option<String>,
    /// 是否强制跳过缓存重算
    pub refresh: Option<bool>,
}

/// 归档目录查询参数
#[derive(Debug, Clone, Deserialize)]
pub struct SentimentDatesQuery {
    /// 最多返回多少条，默认 60，上限 365
    pub limit: Option<u32>,
}

/// 已归档日报的目录条目
///
/// 只带「决定要不要点开这一天」所需的最小信息，不含完整报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentArchiveEntry {
    /// 交易日 YYYY-MM-DD
    pub date: String,
    /// 归档时间戳（秒）
    pub generated_at: i64,
    /// payload 字节数
    pub payload_bytes: usize,
    /// 纳入统计的个股家数
    pub covered_count: u32,
    /// 行业板块数
    pub board_count: usize,
    /// 服务端是否已留存导出件（`data/reports/sentiment-{date}.html`）
    ///
    /// 由路由层按文件系统实际情况填写——仓库层不碰磁盘，所以查库拿到的
    /// 这个值恒为 `false`，需调用方覆盖。
    pub has_export: bool,
}

/// 已归档日报目录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentArchiveList {
    /// 总条数（limit 截断后）
    pub total: usize,
    pub entries: Vec<SentimentArchiveEntry>,
    /// 自动归档任务最近一次运行记录
    ///
    /// 无人值守的定时任务最怕「悄悄坏了没人知道」：失败只写日志的话，用户看到
    /// 的只是「归档没涨」，根本不知道该去哪查。带上它，前端能在抽屉顶部直接
    /// 给出状态。未开启自动归档时为 `None`。
    pub last_run: Option<JobRunInfo>,
}
