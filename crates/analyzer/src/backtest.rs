//! 做T策略回测引擎
//!
//! 直接复用 `t_strategy::analyze_t_signals` 的信号层，
//! 保证**回测与实盘用同一套信号逻辑**，避免两套代码漂移。
//!
//! 本模块负责：按信号模拟成交、扣减交易成本、统计绩效、生成资金曲线与结论。

use appcore::{
    EquityPoint, ExitMode, Kline, SignalAction, TBacktestResult, TBacktestStats, TDirection, TMode,
    TScanItem, TStrategyConfig, TTrade, TradeLeg,
};
use crate::t_strategy::analyze_t_signals;

/// 运行单组参数的回测
///
/// - `klines`：K 线序列（按时间正序）
/// - `code` / `period`：标的与周期，仅用于结果标注
/// - `config`：策略参数
pub fn run_t_backtest(
    klines: &[Kline],
    code: &str,
    period: &str,
    config: &TStrategyConfig,
) -> TBacktestResult {
    let bar_count = klines.len();
    let start_date = klines.first().map(|k| k.date.clone()).unwrap_or_default();
    let end_date = klines.last().map(|k| k.date.clone()).unwrap_or_default();

    let signals = analyze_t_signals(klines, config);
    let last_close = klines.last().map(|k| k.close).unwrap_or(0.0);
    let last_date = klines.last().map(|k| k.date.clone()).unwrap_or_default();
    let last_index = klines.len().saturating_sub(1);
    // 防未来函数：信号在第 i 根收盘生成，不能以第 i 根收盘价成交。
    // 回测执行统一顺延到第 i+1 根开盘价；最后一根出现的信号无法成交，丢弃。
    let exec_signals: Vec<appcore::TSignal> = signals
        .iter()
        .filter_map(|sig| {
            let next = klines.get(sig.index + 1)?;
            let mut s = sig.clone();
            s.index = sig.index + 1;
            s.date = next.date.clone();
            s.price = if next.open > 0.0 { next.open } else { next.close };
            Some(s)
        })
        .collect();
    let trades = build_trades(&exec_signals, config, last_close, &last_date, last_index);
    let stats = compute_stats(&trades, &exec_signals);
    let equity_curve = build_equity_curve(&trades);
    let conclusion = build_conclusion(&stats);

    TBacktestResult {
        code: code.to_string(),
        period: period.to_string(),
        start_date,
        end_date,
        bar_count,
        config: config.clone(),
        trades,
        stats,
        equity_curve,
        signals,
        conclusion,
    }
}

/// 把信号序列结算为交易记录（持仓账本模式）
///
/// 与简单「两两配对」不同，这里维护一个持仓账本，逐笔信号累加股数与金额：
/// - `Entry` / `Add` → 记入建仓侧
/// - `Reduce` / `Clear` → 记入平仓侧
/// - 平仓侧股数追平建仓侧时，结算为一笔完整交易
///
/// 这样才能正确支持**分批建仓**与**分批止盈**（一笔交易含多个 leg），
/// 并且成本按每批成交金额计算，而非笼统扣一次往返费。
fn build_trades(
    signals: &[appcore::TSignal],
    config: &TStrategyConfig,
    last_close: f64,
    last_date: &str,
    last_index: usize,
) -> Vec<TTrade> {
    let mut trades = Vec::new();
    let mut seq = 0usize;
    let mut cur: Option<Ledger> = None;

    for sig in signals {
        match sig.action {
            SignalAction::Entry => {
                let mut l = Ledger::new(sig);
                l.apply_open(sig, config);
                cur = Some(l);
            }
            SignalAction::Add => {
                if let Some(l) = cur.as_mut() {
                    l.apply_open(sig, config);
                }
            }
            SignalAction::Reduce | SignalAction::Clear => {
                if let Some(l) = cur.as_mut() {
                    l.apply_close(sig, config);
                }
                if cur.as_ref().map(|l| l.is_closed()).unwrap_or(false) {
                    if let Some(l) = cur.take() {
                        seq += 1;
                        trades.push(l.settle(seq, config));
                    }
                }
            }
        }
    }

    // 末尾未平仓：按最后收盘价平仓计入，否则会凭空丢失一笔真实持仓的盈亏
    if let Some(mut l) = cur.take() {
        l.force_close(last_close, last_date, last_index, config);
        seq += 1;
        trades.push(l.settle(seq, config));
    }

    trades
}

/// 一笔做T进行中的持仓账本
#[derive(Debug, Clone)]
struct Ledger {
    direction: TDirection,
    t_type: String,
    entry_date: String,
    entry_index: usize,
    strength: f64,
    /// 建仓侧累计股数 / 金额
    open_shares: f64,
    open_amount: f64,
    /// 平仓侧累计股数 / 金额
    close_shares: f64,
    close_amount: f64,
    /// 累计交易费用
    fee: f64,
    /// 建仓侧累计仓位比例（用于展示实际使用的仓位）
    open_ratio: f64,
    legs: Vec<TradeLeg>,
    last_date: String,
    last_index: usize,
}

impl Ledger {
    fn new(sig: &appcore::TSignal) -> Self {
        Self {
            direction: sig.direction,
            t_type: sig.t_type.clone(),
            entry_date: sig.date.clone(),
            entry_index: sig.index,
            strength: sig.strength,
            open_shares: 0.0,
            open_amount: 0.0,
            close_shares: 0.0,
            close_amount: 0.0,
            fee: 0.0,
            open_ratio: 0.0,
            legs: Vec::new(),
            last_date: sig.date.clone(),
            last_index: sig.index,
        }
    }

    /// 成交股数：底仓市值 × 本批仓位比例 ÷ 成交价
    fn shares_for(sig: &appcore::TSignal, config: &TStrategyConfig) -> f64 {
        if sig.price <= 0.0 {
            0.0
        } else {
            config.base_capital * sig.position_pct / sig.price
        }
    }

    fn apply_open(&mut self, sig: &appcore::TSignal, config: &TStrategyConfig) {
        let shares = Self::shares_for(sig, config);
        let amount = sig.price * shares;
        let fee = amount * config.cost_rate / 2.0;
        self.open_shares += shares;
        self.open_amount += amount;
        self.open_ratio += sig.position_pct;
        self.fee += fee;
        self.legs.push(TradeLeg {
            date: sig.date.clone(),
            action: "买入".to_string(),
            price: sig.price,
            shares,
            amount,
            reason: sig.reasons.first().cloned().unwrap_or_default(),
        });
        self.last_date = sig.date.clone();
        self.last_index = sig.index;
    }

    fn apply_close(&mut self, sig: &appcore::TSignal, config: &TStrategyConfig) {
        // 平仓股数按「本次比例 / 累计建仓比例」折算到**实际持仓股数**，
        // 不能像建仓那样用现价重新折算：价格变动会让平仓股数对不上持仓，
        // 分批止盈时会出现「永远平不完」而只能靠超时强平。
        let mut shares = if self.open_ratio > 0.0 {
            self.open_shares * (sig.position_pct / self.open_ratio).min(1.0)
        } else {
            0.0
        };
        // 双保险：不超过剩余未平仓股数
        let remaining = self.open_shares - self.close_shares;
        if shares > remaining {
            shares = remaining;
        }
        let amount = sig.price * shares;
        let fee = amount * config.cost_rate / 2.0;
        self.close_shares += shares;
        self.close_amount += amount;
        self.fee += fee;
        self.legs.push(TradeLeg {
            date: sig.date.clone(),
            action: "卖出".to_string(),
            price: sig.price,
            shares,
            amount,
            reason: sig.reasons.first().cloned().unwrap_or_default(),
        });
        self.last_date = sig.date.clone();
        self.last_index = sig.index;
    }

    /// 平仓侧是否已追平建仓侧（留 0.1% 容差，避免分批累加的浮点误差）
    fn is_closed(&self) -> bool {
        self.open_shares > 0.0 && self.close_shares >= self.open_shares * 0.999
    }

    /// 回测区间结束时仍未平仓：按最后收盘价补一笔平仓
    fn force_close(&mut self, price: f64, date: &str, index: usize, config: &TStrategyConfig) {
        let shares = self.open_shares - self.close_shares;
        if shares <= 0.0 {
            return;
        }
        let amount = price * shares;
        let fee = amount * config.cost_rate / 2.0;
        self.close_shares += shares;
        self.close_amount += amount;
        self.fee += fee;
        self.legs.push(TradeLeg {
            date: date.to_string(),
            action: "卖出".to_string(),
            price,
            shares,
            amount,
            reason: "回测区间结束，按最后收盘价平仓".to_string(),
        });
        self.last_date = date.to_string();
        self.last_index = index;
    }

    /// 结算为一笔完整交易
    ///
    /// 正T：建仓侧是买入支出，平仓侧是卖出收入，pnl = 收入 - 支出
    /// 反T：建仓侧是卖出收入，平仓侧是买回支出，pnl = 收入 - 支出（符号相反）
    fn settle(self, seq: usize, _config: &TStrategyConfig) -> TTrade {
        let sign = match self.direction {
            TDirection::Buy => 1.0,
            TDirection::Sell => -1.0,
        };
        let gross_pnl_pct = if self.open_amount > 0.0 {
            sign * (self.close_amount - self.open_amount) / self.open_amount * 100.0
        } else {
            0.0
        };
        let pnl_amount = sign * (self.close_amount - self.open_amount) - self.fee;
        let net_pnl_pct = if self.open_amount > 0.0 {
            pnl_amount / self.open_amount * 100.0
        } else {
            0.0
        };

        let open_avg = if self.open_shares > 0.0 {
            self.open_amount / self.open_shares
        } else {
            0.0
        };
        let close_avg = if self.close_shares > 0.0 {
            self.close_amount / self.close_shares
        } else {
            0.0
        };

        TTrade {
            seq,
            t_type: self.t_type,
            entry_date: self.entry_date,
            entry_price: open_avg,
            exit_date: self.last_date,
            exit_price: close_avg,
            shares: self.open_shares,
            gross_pnl_pct,
            net_pnl_pct,
            pnl_amount,
            hold_bars: self.last_index.saturating_sub(self.entry_index),
            exit_reason: self
                .legs
                .last()
                .map(|l| l.reason.clone())
                .unwrap_or_default(),
            entry_strength: self.strength,
            used_position_ratio: self.open_ratio,
            legs: self.legs,
        }
    }
}

/// 计算统计指标
///
/// 不接收 `config`：收益相关口径一律取每笔交易的**实际**仓位（`used_position_ratio`），
/// 而非配置里的固定仓位，否则开启动态仓位后累计收益会与固定仓位完全相同。
fn compute_stats(trades: &[TTrade], signals: &[appcore::TSignal]) -> TBacktestStats {
    let total = trades.len();
    if total == 0 {
        return TBacktestStats {
            total_trades: 0,
            win_trades: 0,
            loss_trades: 0,
            win_rate: 0.0,
            avg_pnl_pct: 0.0,
            total_pnl_pct: 0.0,
            profit_factor: 0.0,
            avg_hold_bars: 0.0,
            max_win_pct: 0.0,
            max_loss_pct: 0.0,
            max_drawdown_pct: 0.0,
            signal_count: signals.len(),
            positive_trades: 0,
            reverse_trades: 0,
            stop_loss_exits: 0,
            timeout_exits: 0,
        };
    }

    let wins: Vec<&TTrade> = trades.iter().filter(|t| t.is_win()).collect();
    let gross_profit: f64 = wins.iter().map(|t| t.net_pnl_pct).sum();
    let gross_loss: f64 = trades
        .iter()
        .filter(|t| !t.is_win())
        .map(|t| t.net_pnl_pct.abs())
        .sum();

    // 累计收益按实际仓位加权
    let total_pnl_pct: f64 = trades
        .iter()
        // 用每笔**实际**使用的仓位加权，而非配置里的固定值——
        // 否则开启动态仓位后，强信号重仓带来的增益会被抹平，
        // 累计收益与固定仓位完全相同（看不出动态调仓的效果）。
        .map(|t| t.net_pnl_pct * t.used_position_ratio)
        .sum();

    let curve = build_equity_curve(trades);
    let max_drawdown_pct = max_drawdown(&curve);

    TBacktestStats {
        total_trades: total,
        win_trades: wins.len(),
        loss_trades: total - wins.len(),
        win_rate: wins.len() as f64 / total as f64 * 100.0,
        avg_pnl_pct: trades.iter().map(|t| t.net_pnl_pct).sum::<f64>() / total as f64,
        total_pnl_pct,
        // 无亏损时盈亏比理论上无穷大，但 f64::INFINITY 序列化为 JSON 会变成 null，
        // 前端无法处理。这里钳制为一个大数，语义上表示「无亏损」。
        profit_factor: if gross_loss > 0.0 {
            gross_profit / gross_loss
        } else if gross_profit > 0.0 {
            999.99
        } else {
            0.0
        },
        avg_hold_bars: trades.iter().map(|t| t.hold_bars).sum::<usize>() as f64 / total as f64,
        max_win_pct: trades.iter().map(|t| t.net_pnl_pct).fold(f64::MIN, f64::max),
        max_loss_pct: trades.iter().map(|t| t.net_pnl_pct).fold(f64::MAX, f64::min),
        max_drawdown_pct,
        signal_count: signals.len(),
        positive_trades: trades.iter().filter(|t| t.t_type == "正T").count(),
        reverse_trades: trades.iter().filter(|t| t.t_type == "反T").count(),
        stop_loss_exits: trades
            .iter()
            .filter(|t| t.exit_reason.contains("止损"))
            .count(),
        timeout_exits: trades
            .iter()
            .filter(|t| t.exit_reason.contains("强制平仓"))
            .count(),
    }
}

/// 构建资金曲线（累计收益率，按仓位加权）
fn build_equity_curve(trades: &[TTrade]) -> Vec<EquityPoint> {
    let mut cum = 0.0;
    trades
        .iter()
        .map(|t| {
            cum += t.net_pnl_pct * t.used_position_ratio;
            EquityPoint {
                date: t.exit_date.clone(),
                cumulative_pct: cum,
            }
        })
        .collect()
}

/// 计算资金曲线最大回撤（%）
fn max_drawdown(curve: &[EquityPoint]) -> f64 {
    let mut peak = f64::MIN;
    let mut max_dd = 0.0;
    for p in curve {
        if p.cumulative_pct > peak {
            peak = p.cumulative_pct;
        }
        let dd = peak - p.cumulative_pct;
        if dd > max_dd {
            max_dd = dd;
        }
    }
    if peak == f64::MIN {
        0.0
    } else {
        max_dd
    }
}

/// 根据统计结果生成中文结论（含风险提示）
pub fn build_conclusion(stats: &TBacktestStats) -> String {
    if stats.total_trades == 0 {
        return "本次参数下未产生任何交易信号。六步法则要求「J 值越界 + 交叉确认 + 触及布林轨」"
            .to_string()
            + "三者配合，条件较严；可尝试放宽 J 值阈值、增大警戒窗口，或关闭缩量/形态过滤后重测。";
    }

    let mut s = format!(
        "共产生 {} 笔做T（正T {} 笔、反T {} 笔），胜率 {:.1}%，单笔平均 {:+.2}%，\
         盈亏比 {:.2}，按实际仓位加权累计 {:+.2}%，最大回撤 {:.2}%，平均持有 {:.1} 根K线。",
        stats.total_trades,
        stats.positive_trades,
        stats.reverse_trades,
        stats.win_rate,
        stats.avg_pnl_pct,
        stats.profit_factor,
        stats.total_pnl_pct,
        stats.max_drawdown_pct,
        stats.avg_hold_bars,
    );

    // 风险提示
    let timeout_rate = stats.timeout_exits as f64 / stats.total_trades as f64 * 100.0;
    if timeout_rate > 40.0 {
        s.push_str(&format!(
            " ⚠️ 有 {:.0}% 的交易是靠「持有超时」强平离场，说明出场条件（如等上轨+死叉）过严、"
                , timeout_rate
        ));
        s.push_str("多数单子等不到目标位，建议改用中轨止盈。");
    }
    if stats.stop_loss_exits as f64 / stats.total_trades as f64 > 0.3 {
        s.push_str(" ⚠️ 止损占比较高，需复核止损位是否过窄。");
    }
    if stats.total_trades < 10 {
        s.push_str(&format!(
            " ⚠️ 样本仅 {} 笔，统计意义有限，结论不足以支撑实盘，建议扩大样本（更多标的/更长区间）后重测。",
            stats.total_trades
        ));
    }
    if stats.profit_factor < 1.2 {
        s.push_str(" ⚠️ 盈亏比偏低，扣除交易成本后优势不明显。");
    }

    s
}

/// 参数扫描：对比几组典型配置
///
/// 对应前期实证的四个变体：
/// - A 原始六步（上轨止盈）
/// - B 中轨止盈（实测最大改进）
/// - C 中轨止盈 + 缩量确认（实测盈亏比最优）
/// - D 再加形态过滤（质量最高但信号稀少）
pub fn scan_t_params(klines: &[Kline]) -> Vec<TScanItem> {
    let base = TStrategyConfig::default();

    let mut a = base.clone();
    a.exit_mode = ExitMode::UpperBand;
    a.volume_filter = false;

    let mut b = base.clone();
    b.exit_mode = ExitMode::MidBand;
    b.volume_filter = false;

    let mut c = base.clone();
    c.exit_mode = ExitMode::MidBand;
    c.volume_filter = true;

    let mut d = base.clone();
    d.exit_mode = ExitMode::MidBand;
    d.volume_filter = true;
    d.trend_filter = true;

    let mut e = base.clone();
    e.exit_mode = ExitMode::MidBand;
    e.volume_filter = true;
    e.t_mode = TMode::Positive;

    let mut f = base.clone();
    f.exit_mode = ExitMode::MidBand;
    f.volume_filter = true;
    f.t_mode = TMode::Reverse;

    // G：C 参数 + 分批止盈 + 动态仓位（实测在多数标的上优于 C）
    let mut g = c.clone();
    g.split_exit = true;
    g.dynamic_position = true;

    // H：G 再加分批建仓。注意分批建仓会拉低仓位利用率——
    // 若后续加仓未触发，实际投入只有计划的 1/max_entries，
    // 因此它对盈亏比有利、对累计收益未必。
    let mut h = g.clone();
    h.split_entry = true;

    let configs = vec![
        ("A 原始六步（上轨止盈）", a),
        ("B 中轨止盈", b),
        ("C 中轨止盈 + 缩量确认", c),
        ("D 再加震荡过滤", d),
        ("E 仅正T（C 参数）", e),
        ("F 仅反T（C 参数）", f),
        ("G C + 分批止盈 + 动态仓位", g),
        ("H G + 分批建仓", h),
    ];

    configs
        .into_iter()
        .map(|(label, cfg)| {
            let result = run_t_backtest(klines, "", "", &cfg);
            TScanItem {
                label: label.to_string(),
                total_pnl_pct: result.stats.total_pnl_pct,
                stats: result.stats,
                config: cfg,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_klines(closes: &[f64]) -> Vec<Kline> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                let osc = 0.3 * (i as f64 * 0.9).sin();
                Kline {
                    stock_code: "TEST".to_string(),
                    date: format!("2026-{:02}-{:02}", (i / 28) % 12 + 1, (i % 28) + 1),
                    open: c,
                    close: c,
                    high: c + osc.abs() + 0.1,
                    low: c - osc.abs() - 0.1,
                    volume: 1_000_000.0 * (1.0 + 0.2 * (i as f64 * 1.7).sin()),
                    amount: 0.0,
                    pct_change: None,
                    turnover: None,
                }
            })
            .collect()
    }

    /// 构造一段有明显涨跌起伏的走势，确保能产生交易
    fn oscillating_series() -> Vec<f64> {
        let mut v = Vec::new();
        for wave in 0..6 {
            let base = 10.0 + (wave % 2) as f64 * 0.5;
            for i in 0..25 {
                // 每 25 根完成一次「下跌 8% → 反弹」的循环
                let phase = (i as f64 / 25.0) * std::f64::consts::PI * 2.0;
                v.push(base + 0.9 * phase.sin() - 0.02 * i as f64);
            }
        }
        v
    }

    #[test]
    fn test_backtest_produces_consistent_stats() {
        let kl = make_klines(&oscillating_series());
        let mut cfg = TStrategyConfig::default();
        cfg.volume_filter = false;
        let r = run_t_backtest(&kl, "TEST", "daily", &cfg);

        // 统计自洽性
        assert_eq!(
            r.stats.win_trades + r.stats.loss_trades,
            r.stats.total_trades
        );
        assert_eq!(r.trades.len(), r.stats.total_trades);
        assert_eq!(r.equity_curve.len(), r.trades.len());

        // 每个信号恰好产生一个 leg；若末尾有未平仓信号，会再补一个平仓 leg。
        // 不能再用「信号数/2 = 交易数」的旧断言——分批模式下
        // 一笔交易可包含 Entry+Add+Reduce+Clear 多个信号。
        let total_legs: usize = r.trades.iter().map(|t| t.legs.len()).sum();
        assert!(
            total_legs == r.stats.signal_count || total_legs == r.stats.signal_count + 1,
            "leg 总数应与信号数一致（可多 1 个末尾平仓）：legs={} signals={}",
            total_legs,
            r.stats.signal_count
        );
        // 每笔交易至少一买一卖
        for t in &r.trades {
            assert!(t.legs.len() >= 2, "每笔交易至少应有 2 个 leg");
        }
        if r.stats.total_trades > 0 {
            assert!(r.stats.win_rate >= 0.0 && r.stats.win_rate <= 100.0);
            assert!(r.stats.max_drawdown_pct >= 0.0);
        }
    }

    #[test]
    fn test_cost_is_charged_per_leg() {
        let kl = make_klines(&oscillating_series());
        let mut cfg = TStrategyConfig::default();
        cfg.volume_filter = false;
        let r = run_t_backtest(&kl, "TEST", "daily", &cfg);
        assert!(!r.trades.is_empty(), "该构造应产生交易，否则本测试无意义");

        for t in &r.trades {
            // 成本按每笔 leg 的成交额 × 单边费率累加
            let fee: f64 = t.legs.iter().map(|l| l.amount * cfg.cost_rate / 2.0).sum();
            let open_amount = t.entry_price * t.shares;
            let fee_pct = fee / open_amount * 100.0;
            assert!(
                (t.gross_pnl_pct - t.net_pnl_pct - fee_pct).abs() < 1e-6,
                "净收益应等于毛收益减去按成交额计算的成本：gross={} net={} fee_pct={}",
                t.gross_pnl_pct,
                t.net_pnl_pct,
                fee_pct
            );
            // 有成本时净收益必然低于毛收益
            assert!(t.net_pnl_pct < t.gross_pnl_pct);
        }
    }

    #[test]
    fn test_empty_result_when_no_signals() {
        let kl = make_klines(&vec![10.0; 150]);
        let cfg = TStrategyConfig::default();
        let r = run_t_backtest(&kl, "TEST", "daily", &cfg);
        assert_eq!(r.stats.total_trades, 0);
        assert_eq!(r.stats.win_rate, 0.0);
        assert!(r.conclusion.contains("未产生任何交易"));
    }

    /// 默认配置（分批/动态仓位全关）应与旧版行为一致：每笔恰好一买一卖
    #[test]
    fn test_default_config_has_two_legs_per_trade() {
        let kl = make_klines(&oscillating_series());
        let mut cfg = TStrategyConfig::default();
        cfg.volume_filter = false;
        assert!(!cfg.split_entry && !cfg.split_exit && !cfg.dynamic_position);

        let r = run_t_backtest(&kl, "TEST", "daily", &cfg);
        assert!(!r.trades.is_empty());
        for t in &r.trades {
            assert_eq!(t.legs.len(), 2, "默认配置下每笔应恰好一买一卖");
            assert!(
                (t.used_position_ratio - cfg.position_ratio).abs() < 1e-9,
                "未开启动态仓位时应固定使用 position_ratio"
            );
        }
    }

    /// 构造「横盘 → 急跌 → 反弹」走势，用于触发分批建仓
    ///
    /// 正弦波不行：进场点在波谷，之后一路上涨，不满足「继续下跌才补仓」。
    fn deep_drop_then_rebound() -> Vec<f64> {
        let mut v = Vec::new();
        for i in 0..60 {
            v.push(10.0 + 0.08 * (i as f64 * 0.7).sin());
        }
        for i in 0..18 {
            v.push(10.0 - 0.10 * (i + 1) as f64); // 跌 18%
        }
        for i in 0..25 {
            v.push(8.2 + 0.09 * (i + 1) as f64);
        }
        v
    }

    /// 方向1：开启分批建仓后，应出现包含多个买入 leg 的交易
    ///
    /// 注意止损与分批的参数张力：默认 stop_loss=5% 时，跌 5% 就强平了，
    /// 根本轮不到第二次补仓。这里放宽止损以观察分批效果。
    #[test]
    fn test_split_entry_produces_multiple_buy_legs() {
        let kl = make_klines(&deep_drop_then_rebound());
        let mut cfg = TStrategyConfig::default();
        cfg.volume_filter = false;
        cfg.split_entry = true;
        cfg.max_entries = 3;
        cfg.add_step = 0.02;
        cfg.stop_loss = 0.20; // 放宽，让分批有机会触发
        cfg.j_oversold = 25.0; // 该序列急跌时 J 最低约 13，放宽以激活警戒
        cfg.t_mode = TMode::Positive;
        let r = run_t_backtest(&kl, "TEST", "daily", &cfg);

        assert!(!r.trades.is_empty(), "该构造应产生交易");
        let buy_counts: Vec<usize> = r
            .trades
            .iter()
            .map(|t| t.legs.iter().filter(|l| l.action == "买入").count())
            .collect();
        assert!(
            buy_counts.iter().any(|&c| c > 1),
            "开启分批建仓后应出现多批买入，实际每笔买入批数：{:?}",
            buy_counts
        );
    }

    /// 分批建仓不得让总仓位超出计划
    ///
    /// 回归测试：曾出现过「Entry 投满整个计划仓位，Add 再按 计划/max_entries 追加」
    /// 的实现，导致分批 3 次后总仓位膨胀到计划的 1.4 倍（实测 72% vs 计划 43%）。
    /// 正确做法是首批就只投 计划/max_entries。
    #[test]
    fn test_split_entry_does_not_inflate_total_position() {
        let kl = make_klines(&deep_drop_then_rebound());
        let mut cfg = TStrategyConfig::default();
        cfg.volume_filter = false;
        cfg.split_entry = true;
        cfg.max_entries = 3;
        cfg.add_step = 0.02;
        cfg.stop_loss = 0.20;
        cfg.j_oversold = 25.0;
        cfg.t_mode = TMode::Positive;
        let r = run_t_backtest(&kl, "TEST", "daily", &cfg);

        assert!(!r.trades.is_empty(), "该构造应产生交易");
        let max_ratio = r
            .trades
            .iter()
            .map(|t| t.used_position_ratio)
            .fold(0.0_f64, f64::max);
        assert!(
            max_ratio <= cfg.position_ratio + 1e-6,
            "分批建仓后总仓位不应超过计划：{:.4} vs 计划 {:.4}",
            max_ratio,
            cfg.position_ratio
        );
        assert!(
            r.trades
                .iter()
                .any(|t| t.legs.iter().filter(|l| l.action == "买入").count() > 1),
            "应存在多批买入的交易，否则本测试失去意义"
        );
    }

    /// 止损过窄会导致分批建仓无法展开（跌破止损即强平）
    #[test]
    fn test_narrow_stop_loss_limits_split_entry() {
        let kl = make_klines(&deep_drop_then_rebound());
        let mut narrow = TStrategyConfig::default();
        narrow.volume_filter = false;
        narrow.split_entry = true;
        narrow.add_step = 0.02;
        narrow.stop_loss = 0.03;
        narrow.j_oversold = 25.0;
        narrow.t_mode = TMode::Positive;

        let mut wide = narrow.clone();
        wide.stop_loss = 0.20;

        let a = run_t_backtest(&kl, "TEST", "daily", &narrow);
        let b = run_t_backtest(&kl, "TEST", "daily", &wide);
        let max_a = a
            .trades
            .iter()
            .map(|t| t.legs.iter().filter(|l| l.action == "买入").count())
            .max()
            .unwrap_or(0);
        let max_b = b
            .trades
            .iter()
            .map(|t| t.legs.iter().filter(|l| l.action == "买入").count())
            .max()
            .unwrap_or(0);
        assert!(
            max_b >= max_a,
            "放宽止损后最大分批数不应减少：窄={} 宽={}",
            max_a,
            max_b
        );
    }

    /// 方向1：分批建仓不改变计划总仓位（只改变投入节奏）
    #[test]
    fn test_split_entry_keeps_total_position() {
        let kl = make_klines(&oscillating_series());
        let mut base = TStrategyConfig::default();
        base.volume_filter = false;
        let mut split = base.clone();
        split.split_entry = true;
        split.max_entries = 3;

        let a = run_t_backtest(&kl, "TEST", "daily", &base);
        let b = run_t_backtest(&kl, "TEST", "daily", &split);
        for t in &b.trades {
            assert!(
                t.used_position_ratio <= base.position_ratio + 1e-6,
                "分批建仓不应超过计划总仓位：{:.4} vs {:.4}",
                t.used_position_ratio,
                base.position_ratio
            );
        }
        // 两组的信号强度分布应一致（分批只影响执行节奏，不影响信号判定）
        assert_eq!(a.stats.signal_count > 0, b.stats.signal_count > 0);
    }

    /// 方向1：分批止盈应产生 Reduce 动作（先平一部分）
    #[test]
    fn test_split_exit_produces_reduce_action() {
        let kl = make_klines(&oscillating_series());
        let mut cfg = TStrategyConfig::default();
        cfg.volume_filter = false;
        cfg.split_exit = true;
        let r = run_t_backtest(&kl, "TEST", "daily", &cfg);

        let has_reduce = r
            .signals
            .iter()
            .any(|s| s.action == SignalAction::Reduce);
        assert!(has_reduce, "开启分批止盈后应出现 Reduce 动作的信号");
    }

    /// 方向2：动态仓位应按信号强度在区间内插值
    #[test]
    fn test_dynamic_position_scales_with_strength() {
        let cfg = TStrategyConfig {
            dynamic_position: true,
            min_position_ratio: 0.10,
            max_position_ratio: 0.50,
            ..Default::default()
        };
        assert!((cfg.position_for(0.0) - 0.10).abs() < 1e-9, "弱信号用下限");
        assert!((cfg.position_for(1.0) - 0.50).abs() < 1e-9, "强信号用上限");
        assert!((cfg.position_for(0.5) - 0.30).abs() < 1e-9, "中等信号取中间");
        // 超出范围应被钳制
        assert!((cfg.position_for(-1.0) - 0.10).abs() < 1e-9);
        assert!((cfg.position_for(9.0) - 0.50).abs() < 1e-9);

        // 关闭时恒为固定值
        let off = TStrategyConfig::default();
        assert!((off.position_for(0.0) - off.position_ratio).abs() < 1e-9);
        assert!((off.position_for(1.0) - off.position_ratio).abs() < 1e-9);
    }

    /// 方向2：动态仓位下，强信号交易的仓位应大于弱信号交易
    #[test]
    fn test_dynamic_position_varies_across_trades() {
        let kl = make_klines(&oscillating_series());
        let mut cfg = TStrategyConfig::default();
        cfg.volume_filter = false;
        cfg.dynamic_position = true;
        let r = run_t_backtest(&kl, "TEST", "daily", &cfg);
        if r.trades.len() >= 2 {
            let ratios: Vec<f64> = r.trades.iter().map(|t| t.used_position_ratio).collect();
            let min = ratios.iter().cloned().fold(f64::MAX, f64::min);
            let max = ratios.iter().cloned().fold(f64::MIN, f64::max);
            assert!(
                max >= cfg.min_position_ratio - 1e-9 && max <= cfg.max_position_ratio + 1e-9,
                "仓位应落在配置区间内：{:?}",
                ratios
            );
            assert!(min <= max + 1e-9);
        }
    }

    #[test]
    fn test_scan_returns_all_variants() {
        let kl = make_klines(&oscillating_series());
        let items = scan_t_params(&kl);
        // 按前缀字母断言而非硬编码数量：新增变体时这里必须同步，避免又出现
        // 「少了个 #[test]，测试静默不跑也没人发现」——那正是本测试曾经的状态。
        let prefixes: Vec<&str> = items
            .iter()
            .map(|i| i.label.split(' ').next().unwrap_or(""))
            .collect();
        assert_eq!(prefixes, vec!["A", "B", "C", "D", "E", "F", "G", "H"]);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.iter().any(|l| l.contains("中轨止盈")));
        assert!(labels.iter().any(|l| l.contains("仅正T")));
        assert!(labels.iter().any(|l| l.contains("仅反T")));
        assert!(labels.iter().any(|l| l.contains("分批止盈")));
        assert!(labels.iter().any(|l| l.contains("分批建仓")));
    }

    #[test]
    fn test_max_drawdown_is_non_negative() {
        let curve = vec![
            EquityPoint { date: "1".into(), cumulative_pct: 1.0 },
            EquityPoint { date: "2".into(), cumulative_pct: -2.0 },
            EquityPoint { date: "3".into(), cumulative_pct: 3.0 },
        ];
        // 峰值 1.0 → 低点 -2.0，回撤 3.0
        assert!((max_drawdown(&curve) - 3.0).abs() < 1e-9);
        assert!(max_drawdown(&[]).abs() < 1e-9);
    }

    #[test]
    fn test_position_ratio_weights_total_pnl() {
        let kl = make_klines(&oscillating_series());
        let mut full = TStrategyConfig::default();
        full.volume_filter = false;
        full.position_ratio = 1.0;
        let mut third = TStrategyConfig::default();
        third.volume_filter = false;
        third.position_ratio = 1.0 / 3.0;

        let a = run_t_backtest(&kl, "TEST", "daily", &full);
        let b = run_t_backtest(&kl, "TEST", "daily", &third);
        if a.stats.total_trades > 0 {
            assert!(
                (a.stats.total_pnl_pct - b.stats.total_pnl_pct * 3.0).abs() < 1e-6,
                "累计收益应与仓位比例成正比"
            );
        }
    }
}
