//! 做T策略（KDJ + 布林带六步法则）数据模型
//!
//! 设计要点：
//! - 信号层是**纯函数**（输入 K 线序列 + 配置 → 输出信号序列），
//!   同一套代码同时服务于**回测**与**实盘/扫描**，避免两套逻辑漂移。
//! - 六步法则的关键是**时序**关系：J 值超卖只是「激活警戒」，
//!   金叉才是「触发」，二者不能在同日做 AND（实测同日满足概率≈0）。

use serde::{Deserialize, Serialize};

/// 做T方向
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TMode {
    /// 正T：先买后卖（低吸高抛）。上涨/震荡市适用
    Positive,
    /// 反T：先卖后买（高抛低吸）。下跌/震荡市适用
    Reverse,
    /// 双模：按布林形态自动选择（震荡市两者皆可）
    Both,
}

impl TMode {
    pub fn label(&self) -> &'static str {
        match self {
            TMode::Positive => "正T（低吸高抛）",
            TMode::Reverse => "反T（高抛低吸）",
            TMode::Both => "双模（自动切换）",
        }
    }

    /// 是否允许正T
    pub fn allows_positive(&self) -> bool {
        matches!(self, TMode::Positive | TMode::Both)
    }

    /// 是否允许反T
    pub fn allows_reverse(&self) -> bool {
        matches!(self, TMode::Reverse | TMode::Both)
    }
}

/// 出场（止盈）模式
///
/// 实测结论：中轨止盈显著优于上轨止盈（胜率 56%→73%，平均持有 15.6→7.3 日）。
/// 上轨止盈过于贪婪，多数情况下等不到信号，只能超时平仓。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExitMode {
    /// 回到中轨即止盈（推荐）
    MidBand,
    /// 触及上轨 + 死叉才止盈（原始六步法则）
    UpperBand,
    /// 二选一：先到哪个算哪个
    Either,
}

impl ExitMode {
    pub fn label(&self) -> &'static str {
        match self {
            ExitMode::MidBand => "中轨止盈",
            ExitMode::UpperBand => "上轨止盈",
            ExitMode::Either => "中轨或上轨（先到先得）",
        }
    }
}

/// 买卖方向（单笔交易的一侧）
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TDirection {
    /// 买入（正T的建仓 / 反T的买回）
    Buy,
    /// 卖出（反T的建仓 / 正T的了结）
    Sell,
}

impl TDirection {
    pub fn label(&self) -> &'static str {
        match self {
            TDirection::Buy => "买入",
            TDirection::Sell => "卖出",
        }
    }
}

/// 六步法则的当前所处阶段
/// 信号动作（支持分批建仓 / 分批止盈）
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SignalAction {
    /// 首次建仓（开新的一笔做T）
    Entry,
    /// 加仓（已有持仓，价格继续有利时补仓）
    Add,
    /// 减仓（分批止盈的一部分）
    Reduce,
    /// 清仓（平掉剩余全部持仓，结束这笔做T）
    Clear,
}

impl SignalAction {
    pub fn label(&self) -> &'static str {
        match self {
            SignalAction::Entry => "建仓",
            SignalAction::Add => "加仓",
            SignalAction::Reduce => "减仓",
            SignalAction::Clear => "清仓",
        }
    }
}

/// 单笔做T中的一次成交（分批时，一笔交易会包含多个 leg）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeLeg {
    pub date: String,
    /// "买入" / "卖出"
    pub action: String,
    pub price: f64,
    pub shares: f64,
    /// 成交金额（元）
    pub amount: f64,
    /// 触发原因
    pub reason: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SignalStep {
    /// 第一步：确认大方向（布林形态判定）
    TrendCheck,
    /// 第二步：KDJ 进入超卖区（正T）/ 超买区（反T），警戒激活
    AlertArmed,
    /// 第三步：价格触及布林下轨（正T）/ 上轨（反T）
    BandTouched,
    /// 第四步：金叉（正T）/ 死叉（反T）确认 —— 触发信号
    Triggered,
    /// 第五步：等待进入退出区（正T等超买 / 反T等超卖）
    ExitArmed,
    /// 第六步：出场信号
    ExitTriggered,
}

impl SignalStep {
    pub fn label(&self) -> &'static str {
        match self {
            SignalStep::TrendCheck => "① 确认方向",
            SignalStep::AlertArmed => "② 超卖/超买警戒",
            SignalStep::BandTouched => "③ 触及轨道",
            SignalStep::Triggered => "④ 交叉确认（进场）",
            SignalStep::ExitArmed => "⑤ 等待退出区",
            SignalStep::ExitTriggered => "⑥ 出场信号",
        }
    }
}

/// 单条做T信号
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TSignal {
    /// 日期（日线为 YYYY-MM-DD）
    pub date: String,
    /// 在 K 线序列中的下标
    pub index: usize,
    /// 买卖方向
    pub direction: TDirection,
    /// 动作类型（建仓 / 加仓 / 减仓 / 清仓）
    pub action: SignalAction,
    /// 本次操作占底仓的比例（动态仓位时按信号强度插值）
    pub position_pct: f64,
    /// 触发价格（收盘价）
    pub price: f64,
    /// 所处六步阶段
    pub step: SignalStep,
    /// 做T类型：本次信号属于正T还是反T
    pub t_type: String,
    /// 信号强度 0~1（多重信号叠加越高）
    pub strength: f64,
    /// 命中原因（中文）
    pub reasons: Vec<String>,
    /// 布林带形态
    pub boll_shape: String,
    /// 触发时的 J 值
    pub j_value: f64,
    /// 是否伴随 KDJ 背离（加分项）
    pub has_divergence: bool,
    /// 是否缩量（加分项）
    pub volume_shrunk: bool,
}

/// 做T策略配置（全部可配置，便于回测调参）
///
/// `#[serde(default)]` 让调用方可以只传需要覆盖的字段，缺失项用 `Default` 填充——
/// 便于 curl 调试，也避免前后端版本不一致时整包反序列化失败。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TStrategyConfig {
    // ── KDJ ──
    /// KDJ 周期 N，默认 9
    pub kdj_n: usize,
    /// J 值超卖阈值（警戒激活），默认 10
    pub j_oversold: f64,
    /// J 值超买阈值（警戒激活），默认 80
    pub j_overbought: f64,
    /// 警戒激活后的有效窗口（根），默认 8
    ///
    /// 这是本策略的核心参数：J 超卖与金叉几乎不会同日出现，
    /// 必须允许在超卖后的若干根内等待金叉。
    pub signal_window: usize,

    // ── 布林带 ──
    /// 布林周期 N，默认 20
    pub boll_n: usize,
    /// 布林倍数 K，默认 2.0
    pub boll_k: f64,
    /// 判定"触及下轨"的容差：close <= lower * (1 + tol)，默认 0.02
    pub touch_tol: f64,

    // ── 过滤条件 ──
    /// 是否启用布林形态过滤（只在平行震荡做T），默认 false
    ///
    /// 开启后信号质量最高（实测胜率 100%、单笔 +2.11%），
    /// 但信号极其稀少（6 只票 2 年仅 4 次），建议回测对比后再决定。
    pub trend_filter: bool,
    /// 是否启用缩量确认（缩量触轨才有效），默认 true
    ///
    /// 开启后盈亏比从 1.40 提升到 2.64。
    pub volume_filter: bool,
    /// 缩量阈值：成交量 < 5 日均量 * ratio，默认 0.85
    pub volume_ratio: f64,
    /// 是否要求 KDJ 背离作为加分项（不满足也不否决），默认 true
    pub use_divergence: bool,

    // ── 出场 ──
    /// 出场模式
    pub exit_mode: ExitMode,
    /// 止损比例（跌破入场价 * stop_loss 离场），默认 0.05
    pub stop_loss: f64,
    /// 最大持有根数（超时强平），默认 20
    pub max_hold: usize,

    // ── 交易 ──
    // ── 分批建仓（方向1）──
    /// 是否启用分批建仓：建仓后若价格继续朝有利方向运行，按 add_step 分批补仓
    ///
    /// 正T：买入后继续下跌 → 补仓摊低成本
    /// 反T：卖出后继续上涨 → 继续卖出（锁定更高均价）
    pub split_entry: bool,
    /// 最多分几批建仓，默认 3
    pub max_entries: usize,
    /// 每批之间的价格间隔（相对上一笔成交价），默认 2%
    pub add_step: f64,

    // ── 分批止盈（方向1）──
    /// 是否启用分批止盈：到中轨先平一部分，剩余等上轨
    pub split_exit: bool,
    /// 首批止盈比例（占本次持仓），默认 50%
    pub first_exit_ratio: f64,

    // ── 动态仓位（方向2）──
    /// 是否按信号强度动态调仓：强信号重仓、弱信号轻仓
    pub dynamic_position: bool,
    /// 动态仓位下限（弱信号），默认 15%
    pub min_position_ratio: f64,
    /// 动态仓位上限（强信号），默认 50%
    pub max_position_ratio: f64,

    /// 做T方向
    pub t_mode: TMode,
    /// 单次做T仓位占底仓比例，默认 1/3（动态仓位关闭时生效）
    pub position_ratio: f64,
    /// 往返交易成本比例（佣金+印花税+过户费），默认 0.0012
    pub cost_rate: f64,
    /// 底仓市值（元），用于折算做T股数与盈亏金额，默认 100 万
    pub base_capital: f64,
}

impl Default for TStrategyConfig {
    fn default() -> Self {
        Self {
            kdj_n: 9,
            j_oversold: 10.0,
            j_overbought: 80.0,
            signal_window: 8,
            boll_n: 20,
            boll_k: 2.0,
            touch_tol: 0.02,
            trend_filter: false,
            volume_filter: true,
            volume_ratio: 0.85,
            use_divergence: true,
            exit_mode: ExitMode::MidBand,
            stop_loss: 0.05,
            max_hold: 20,
            t_mode: TMode::Both,
            position_ratio: 1.0 / 3.0,
            cost_rate: 0.0012,
            base_capital: 1_000_000.0,
            // 以下三项默认关闭，保证与旧版本行为完全一致（向后兼容）
            split_entry: false,
            max_entries: 3,
            add_step: 0.02,
            split_exit: false,
            first_exit_ratio: 0.5,
            dynamic_position: false,
            min_position_ratio: 0.15,
            max_position_ratio: 0.5,
        }
    }
}

impl TStrategyConfig {
    /// 按信号强度计算实际仓位比例
    ///
    /// 未开启动态仓位时固定返回 `position_ratio`；
    /// 开启后在 [min, max] 区间按强度线性插值。
    pub fn position_for(&self, strength: f64) -> f64 {
        if !self.dynamic_position {
            return self.position_ratio;
        }
        let s = strength.clamp(0.0, 1.0);
        self.min_position_ratio + (self.max_position_ratio - self.min_position_ratio) * s
    }
}

/// 单笔做T交易记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TTrade {
    /// 交易序号（从 1 开始）
    pub seq: usize,
    /// 做T类型："正T" / "反T"
    pub t_type: String,
    /// 入场日期
    pub entry_date: String,
    /// 入场价
    pub entry_price: f64,
    /// 出场日期
    pub exit_date: String,
    /// 出场价
    pub exit_price: f64,
    /// 做T股数
    pub shares: f64,
    /// 毛收益率（%，未扣成本）
    pub gross_pnl_pct: f64,
    /// 净收益率（%，已扣往返成本）
    pub net_pnl_pct: f64,
    /// 盈亏金额（元）
    pub pnl_amount: f64,
    /// 持有根数
    pub hold_bars: usize,
    /// 出场原因
    pub exit_reason: String,
    /// 入场信号强度
    pub entry_strength: f64,
    /// 实际使用的仓位比例（动态仓位时会偏离 position_ratio）
    pub used_position_ratio: f64,
    /// 分批成交明细（不分批时长度为 2：一买一卖）
    pub legs: Vec<TradeLeg>,
}

impl TTrade {
    pub fn is_win(&self) -> bool {
        self.net_pnl_pct > 0.0
    }
}

/// 回测统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TBacktestStats {
    /// 总交易笔数
    pub total_trades: usize,
    /// 盈利笔数
    pub win_trades: usize,
    /// 亏损笔数
    pub loss_trades: usize,
    /// 胜率（%）
    pub win_rate: f64,
    /// 单笔平均净收益率（%）
    pub avg_pnl_pct: f64,
    /// 累计净收益率（%，按每笔仓位加权累加）
    pub total_pnl_pct: f64,
    /// 盈亏比（总盈利 / 总亏损），>1 为正期望
    pub profit_factor: f64,
    /// 平均持有根数
    pub avg_hold_bars: f64,
    /// 最大单笔盈利（%）
    pub max_win_pct: f64,
    /// 最大单笔亏损（%）
    pub max_loss_pct: f64,
    /// 最大回撤（%，基于累计收益曲线）
    pub max_drawdown_pct: f64,
    /// 信号总数（含未成交的）
    pub signal_count: usize,
    /// 正T笔数
    pub positive_trades: usize,
    /// 反T笔数
    pub reverse_trades: usize,
    /// 因止损离场笔数
    pub stop_loss_exits: usize,
    /// 因超时离场笔数
    pub timeout_exits: usize,
}

/// 资金曲线上的一点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquityPoint {
    pub date: String,
    /// 累计收益率（%）
    pub cumulative_pct: f64,
}

/// 回测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TBacktestResult {
    /// 股票代码
    pub code: String,
    /// 周期描述
    pub period: String,
    /// 回测区间
    pub start_date: String,
    pub end_date: String,
    /// K 线根数
    pub bar_count: usize,
    /// 使用的配置
    pub config: TStrategyConfig,
    /// 交易明细
    pub trades: Vec<TTrade>,
    /// 统计指标
    pub stats: TBacktestStats,
    /// 资金曲线
    pub equity_curve: Vec<EquityPoint>,
    /// 信号明细（用于前端在 K 线上打点）
    pub signals: Vec<TSignal>,
    /// 一句话结论
    pub conclusion: String,
}

/// 参数扫描的单组结果（用于对比不同参数配置）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TScanItem {
    /// 配置标签（如"中轨止盈+缩量"）
    pub label: String,
    pub config: TStrategyConfig,
    pub stats: TBacktestStats,
    pub total_pnl_pct: f64,
}
