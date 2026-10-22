//! 做T信号层：KDJ + 布林带六步法则（时序状态机）
//!
//! # 为什么必须是「时序」而不是「同日 AND」
//!
//! 六步法则原文第二步「等 J 跌破 0，这时候**别急着进**，**等**金叉」——
//! 这个"等"字说明：J 超卖是**激活警戒**，金叉是**后续触发**，二者是先后关系。
//!
//! 实测（440 个交易日）：J<20 占约 30% 的日子，KDJ 金叉只占 6.6%~7.7%，
//! **两者同日出现仅 0~1 天**。若按字面写成同一天的 `AND`，两年几乎零信号。
//!
//! 因此本模块用状态机实现：
//! ```text
//! J 超卖 (激活警戒) ──[signal_window 根内]──> 金叉 + 触下轨 ──> 正T买入信号
//! J 超买 (激活警戒) ──[signal_window 根内]──> 死叉 + 触上轨 ──> 反T卖出信号
//! ```
//!
//! 本模块是**纯函数**：输入 K 线序列 + 配置，输出成对的买卖信号。
//! 回测引擎与实盘扫描共用同一套逻辑，避免两套代码漂移。

use appcore::{Kline, TSignal, TDirection, TStrategyConfig, ExitMode, SignalAction, SignalStep};
use indicators::{calc_kdj, calc_boll, calc_boll_shape, detect_cross, detect_divergence};
use indicators::{KdjCross, KdjDivergence, BollShape};

/// 背离检测回看根数
const DIVERGENCE_LOOKBACK: usize = 30;
/// 量能均线周期
const VOLUME_MA: usize = 5;

/// 内部持仓状态（支持多批次建仓 / 分批止盈）
#[derive(Debug, Clone)]
struct OpenPosition {
    /// 开仓方向：Buy=正T（低吸待高抛），Sell=反T（高抛待低吸）
    direction: TDirection,
    entry_index: usize,
    /// 加权平均成交价（用于止损与盈亏计算）
    avg_price: f64,
    /// 最近一批的成交价（用于判断加仓间隔）
    last_price: f64,
    entry_strength: f64,
    t_type: String,
    /// 已建仓批次
    entry_count: usize,
    /// 已减仓批次
    exit_count: usize,
    /// 累计建仓比例（占底仓）
    total_ratio: f64,
    /// 当前仍持有的比例（减仓后递减）
    remain_ratio: f64,
}

/// 生成完整的做T信号序列（进场 + 出场成对）
///
/// 返回值按时间顺序排列，相邻两条构成一笔交易（Buy→Sell 为正T，Sell→Buy 为反T）。
pub fn analyze_t_signals(klines: &[Kline], config: &TStrategyConfig) -> Vec<TSignal> {
    let n = klines.len();
    let warmup = n
        .min(config.boll_n.max(config.kdj_n).max(DIVERGENCE_LOOKBACK).max(VOLUME_MA) + 30);
    if n < warmup + 2 {
        return Vec::new();
    }

    // ── 预计算指标 ──
    let highs: Vec<f64> = klines.iter().map(|k| k.high).collect();
    let lows: Vec<f64> = klines.iter().map(|k| k.low).collect();
    let closes: Vec<f64> = klines.iter().map(|k| k.close).collect();
    let dates: Vec<String> = klines.iter().map(|k| k.date.clone()).collect();

    let kdj_offset = config.kdj_n - 1;
    let boll_offset = config.boll_n - 1;

    let kdj = calc_kdj(&highs, &lows, &closes, &dates, config.kdj_n);
    let boll = calc_boll(&closes, &dates, config.boll_n, config.boll_k);
    if kdj.is_empty() || boll.is_empty() {
        return Vec::new();
    }

    let shapes = calc_boll_shape(&boll, 20, 1.5, 1.0, 1.10, 0.85, 250);
    let crosses = detect_cross(&kdj);

    // 背离：需传入与 kdj 对齐的 low/high
    let kdj_lows: Vec<f64> = klines[kdj_offset..].iter().map(|k| k.low).collect();
    let kdj_highs: Vec<f64> = klines[kdj_offset..].iter().map(|k| k.high).collect();
    let divergences = detect_divergence(
        &kdj,
        &kdj_lows,
        &kdj_highs,
        DIVERGENCE_LOOKBACK,
        0.0,
    );

    // 量能均线（按原始 K 线下标）
    let vol_ma: Vec<f64> = (0..n)
        .map(|i| {
            if i + 1 < VOLUME_MA {
                f64::NAN
            } else {
                klines[i + 1 - VOLUME_MA..=i]
                    .iter()
                    .map(|k| k.volume)
                    .sum::<f64>()
                    / VOLUME_MA as f64
            }
        })
        .collect();

    let mut signals: Vec<TSignal> = Vec::new();
    let mut position: Option<OpenPosition> = None;

    for i in warmup..n {
        let ki = i as i64 - kdj_offset as i64;
        let bi = i as i64 - boll_offset as i64;
        if ki < 0 || bi < 0 {
            continue;
        }
        let ki = ki as usize;
        let bi = bi as usize;
        if ki >= kdj.len() || bi >= boll.len() || bi >= shapes.len() {
            continue;
        }

        let price = closes[i];
        let j = kdj[ki].j;
        let cross = crosses[ki];
        let div = divergences[ki];
        let shape = shapes[bi].shape;
        let b = &boll[bi];

        // 警戒窗口：最近 signal_window 根内 J 是否曾越界
        let w_start = ki.saturating_sub(config.signal_window - 1);
        let j_oversold_recent = kdj[w_start..=ki].iter().any(|x| x.j < config.j_oversold);
        let j_overbought_recent = kdj[w_start..=ki].iter().any(|x| x.j > config.j_overbought);

        let volume_shrunk = if vol_ma[i].is_nan() || vol_ma[i] <= 0.0 {
            false
        } else {
            klines[i].volume < vol_ma[i] * config.volume_ratio
        };

        // 窗口内是否曾触及布林轨。
        //
        // 这是**第三个时序陷阱**：急跌时价格紧贴下轨，但此时 KDJ 仍在下行、尚无金叉；
        // 等金叉出现时价格往往已反弹离开下轨。若要求「触轨」与「金叉」同日满足，
        // 会系统性错过最典型的低吸形态。因此触轨同样按「窗口内曾发生」判定。
        let wi = i.saturating_sub(config.signal_window - 1);
        let mut touched_lower_recent = false;
        let mut touched_upper_recent = false;
        // 记录窗口内最近一次触轨时的轨道价位，用于信号描述
        let mut lower_at_touch = b.lower;
        let mut upper_at_touch = b.upper;
        for k in wi..=i {
            let bk = k as i64 - boll_offset as i64;
            if bk < 0 || (bk as usize) >= boll.len() {
                continue;
            }
            let bk = bk as usize;
            if closes[k] <= boll[bk].lower * (1.0 + config.touch_tol) {
                touched_lower_recent = true;
                lower_at_touch = boll[bk].lower;
            }
            if closes[k] >= boll[bk].upper * (1.0 - config.touch_tol) {
                touched_upper_recent = true;
                upper_at_touch = boll[bk].upper;
            }
        }

        // 用快照克隆判断，避免借用冲突（需要在分支内更新 position）
        match position.clone() {
            None => {
                // ── 无持仓：寻找进场信号 ──
                let sig = try_entry(
                    i,
                    &dates[i],
                    price,
                    j,
                    cross,
                    div,
                    shape,
                    lower_at_touch,
                    upper_at_touch,
                    j_oversold_recent,
                    j_overbought_recent,
                    volume_shrunk,
                    touched_lower_recent,
                    touched_upper_recent,
                    config,
                );
                if let Some(s) = sig {
                    let ratio = s.position_pct;
                    position = Some(OpenPosition {
                        direction: s.direction,
                        entry_index: i,
                        avg_price: price,
                        last_price: price,
                        entry_strength: s.strength,
                        t_type: s.t_type.clone(),
                        entry_count: 1,
                        exit_count: 0,
                        total_ratio: ratio,
                        remain_ratio: ratio,
                    });
                    signals.push(s);
                }
            }
            Some(pos) => {
                let held = i - pos.entry_index;

                // ── 优先级 1：止损 / 超时 → 强制清仓 ──
                let forced = force_exit(price, &pos, held, config);

                // ── 优先级 2：加仓（价格继续朝有利方向运行）──
                let adding = if forced.is_none()
                    && config.split_entry
                    && pos.entry_count < config.max_entries
                {
                    try_add(price, &pos, config, b.mid)
                } else {
                    None
                };

                // ── 优先级 3：止盈（减仓 / 清仓）──
                let reducing = if forced.is_none() && adding.is_none() {
                    try_reduce(
                        price,
                        j,
                        cross,
                        j_oversold_recent,
                        j_overbought_recent,
                        b.mid,
                        b.lower,
                        b.upper,
                        &pos,
                        config,
                    )
                } else {
                    None
                };

                if let Some((reason, ratio)) = adding {
                    // ── 加仓 ──
                    let new_avg =
                        weighted_avg(pos.avg_price, pos.remain_ratio, price, ratio);
                    signals.push(TSignal {
                        date: dates[i].clone(),
                        index: i,
                        direction: pos.direction,
                        action: SignalAction::Add,
                        position_pct: ratio,
                        price,
                        step: SignalStep::Triggered,
                        t_type: pos.t_type.clone(),
                        strength: pos.entry_strength,
                        reasons: vec![reason],
                        boll_shape: shape.label().to_string(),
                        j_value: j,
                        has_divergence: div != KdjDivergence::None,
                        volume_shrunk,
                    });
                    position = Some(OpenPosition {
                        avg_price: new_avg,
                        last_price: price,
                        entry_count: pos.entry_count + 1,
                        total_ratio: pos.total_ratio + ratio,
                        remain_ratio: pos.remain_ratio + ratio,
                        ..pos
                    });
                } else if let Some((reason, action, ratio)) = forced.or(reducing) {
                    // ── 减仓 / 清仓 ──
                    let direction = match pos.direction {
                        TDirection::Buy => TDirection::Sell,
                        TDirection::Sell => TDirection::Buy,
                    };
                    let remain_after = (pos.remain_ratio - ratio).max(0.0);
                    signals.push(TSignal {
                        date: dates[i].clone(),
                        index: i,
                        direction,
                        action,
                        position_pct: ratio,
                        price,
                        step: SignalStep::ExitTriggered,
                        t_type: pos.t_type.clone(),
                        strength: pos.entry_strength,
                        reasons: vec![reason],
                        boll_shape: shape.label().to_string(),
                        j_value: j,
                        has_divergence: div != KdjDivergence::None,
                        volume_shrunk,
                    });
                    position = if remain_after <= 1e-9 {
                        None
                    } else {
                        Some(OpenPosition {
                            remain_ratio: remain_after,
                            exit_count: pos.exit_count + 1,
                            ..pos
                        })
                    };
                }
            }
        }
    }

    signals
}

/// 首批建仓的仓位比例
///
/// 启用分批建仓时，首批只投入 `总仓位 / max_entries`，后续由 `try_add` 逐批补足；
/// 若首批就投满，再加仓会让**总仓位超出计划**（实测会膨胀到 1.4 倍以上）。
fn first_batch(config: &TStrategyConfig, strength: f64) -> f64 {
    let total = config.position_for(strength);
    if config.split_entry && config.max_entries > 1 {
        total / config.max_entries as f64
    } else {
        total
    }
}

/// 加权平均价
fn weighted_avg(old_price: f64, old_ratio: f64, new_price: f64, new_ratio: f64) -> f64 {
    let total = old_ratio + new_ratio;
    if total <= 0.0 {
        new_price
    } else {
        (old_price * old_ratio + new_price * new_ratio) / total
    }
}

/// 强制清仓：止损或超时。返回 (原因, 清仓比例)
fn force_exit(
    price: f64,
    pos: &OpenPosition,
    held: usize,
    config: &TStrategyConfig,
) -> Option<(String, SignalAction, f64)> {
    // 止损：正T 低吸后继续下跌；反T 高抛后继续上涨（踏空）
    let stop_hit = match pos.direction {
        TDirection::Buy => price < pos.avg_price * (1.0 - config.stop_loss),
        TDirection::Sell => price > pos.avg_price * (1.0 + config.stop_loss),
    };
    if stop_hit {
        return Some((
            format!(
                "触发止损（均价 {:.2} → 现价 {:.2}，{:.2}%）",
                pos.avg_price,
                price,
                (price - pos.avg_price) / pos.avg_price * 100.0
            ),
            SignalAction::Clear,
            pos.remain_ratio,
        ));
    }

    if held >= config.max_hold {
        return Some((
            format!("持有 {} 根达上限，强制平仓", held),
            SignalAction::Clear,
            pos.remain_ratio,
        ));
    }

    None
}

/// 加仓判断：价格继续朝有利方向运行 add_step
///
/// 正T 买入后继续下跌 → 补仓摊低成本；反T 卖出后继续上涨 → 继续卖出锁高价。
/// 每批仓位 = 计划总仓位 / max_entries，**总仓位不变**，只是分批投入。
///
/// 注意过滤条件不能用「J 值仍超卖」：金叉一旦确认，J 值必然已从底部回升，
/// 再要求 J < j_oversold 会让加仓几乎永不触发（与进场逻辑同样的时序陷阱）。
/// 改用「价格仍在中轨的另一侧」——即尚未反转、仍处震荡区间的有利一端。
fn try_add(price: f64, pos: &OpenPosition, config: &TStrategyConfig, mid: f64) -> Option<(String, f64)> {
    let (moved_enough, still_in_zone) = match pos.direction {
        // 正T：再跌 add_step，且仍在中轨下方（尚未反转）
        TDirection::Buy => (
            price <= pos.last_price * (1.0 - config.add_step),
            price < mid,
        ),
        // 反T：再涨 add_step，且仍在中轨上方
        TDirection::Sell => (
            price >= pos.last_price * (1.0 + config.add_step),
            price > mid,
        ),
    };
    if !moved_enough || !still_in_zone {
        return None;
    }

    let per_batch = config.position_for(pos.entry_strength) / config.max_entries as f64;
    Some((
        match pos.direction {
            TDirection::Buy => format!(
                "较上一批 {:.2} 再跌 {:.1}%（现价 {:.2}），第 {} 批补仓摊低成本",
                pos.last_price,
                config.add_step * 100.0,
                price,
                pos.entry_count + 1
            ),
            TDirection::Sell => format!(
                "较上一批 {:.2} 再涨 {:.1}%（现价 {:.2}），第 {} 批加仓锁高均价",
                pos.last_price,
                config.add_step * 100.0,
                price,
                pos.entry_count + 1
            ),
        },
        per_batch,
    ))
}

/// 止盈判断：分批减仓或清仓。返回 (原因, 动作, 比例)
#[allow(clippy::too_many_arguments)]
fn try_reduce(
    price: f64,
    _j: f64,
    cross: KdjCross,
    j_oversold_recent: bool,
    j_overbought_recent: bool,
    mid: f64,
    lower: f64,
    upper: f64,
    pos: &OpenPosition,
    config: &TStrategyConfig,
) -> Option<(String, SignalAction, f64)> {
    // 是否满足「目标位」：中轨（常规）或上轨+死叉（激进）
    let (at_mid, at_upper) = match pos.direction {
        TDirection::Buy => (
            price >= mid,
            price >= upper * 0.98 && cross == KdjCross::Dead && j_overbought_recent,
        ),
        TDirection::Sell => (
            price <= mid,
            price <= lower * 1.02 && cross == KdjCross::Golden && j_oversold_recent,
        ),
    };

    if config.split_exit {
        // 分批止盈：第一批到中轨减仓，剩余等上轨（或下一批仍在中轨之上）再清
        if pos.exit_count == 0 && at_mid {
            let ratio = pos.remain_ratio * config.first_exit_ratio;
            return Some((
                format!(
                    "{}（分批止盈第 1 批，平掉 {:.0}% 仓位）",
                    match pos.direction {
                        TDirection::Buy => format!("价格回到中轨 {:.2}", mid),
                        TDirection::Sell => format!("价格回落至中轨 {:.2}", mid),
                    },
                    config.first_exit_ratio * 100.0
                ),
                SignalAction::Reduce,
                ratio,
            ));
        }
        if pos.exit_count >= 1 && (at_upper || at_mid) {
            return Some((
                format!(
                    "{}（分批止盈第 2 批，清掉剩余仓位）",
                    if at_upper {
                        match pos.direction {
                            TDirection::Buy => "触及上轨且死叉".to_string(),
                            TDirection::Sell => "回落至下轨且金叉".to_string(),
                        }
                    } else {
                        "维持在中轨上方".to_string()
                    }
                ),
                SignalAction::Clear,
                pos.remain_ratio,
            ));
        }
        return None;
    }

    // 不分批：命中出场模式即全部清仓
    let hit = match config.exit_mode {
        ExitMode::MidBand => at_mid,
        ExitMode::UpperBand => at_upper,
        ExitMode::Either => at_mid || at_upper,
    };
    if hit {
        let reason = match pos.direction {
            TDirection::Buy => format!("价格回到中轨 {:.2}，正T止盈", mid),
            TDirection::Sell => format!("价格回落至中轨 {:.2}，反T买回", mid),
        };
        return Some((reason, SignalAction::Clear, pos.remain_ratio));
    }

    None
}

/// 尝试生成进场信号
#[allow(clippy::too_many_arguments)]
fn try_entry(
    index: usize,
    date: &str,
    price: f64,
    j: f64,
    cross: KdjCross,
    div: KdjDivergence,
    shape: BollShape,
    lower: f64,
    upper: f64,
    j_oversold_recent: bool,
    j_overbought_recent: bool,
    volume_shrunk: bool,
    touched_lower_recent: bool,
    touched_upper_recent: bool,
    config: &TStrategyConfig,
) -> Option<TSignal> {
    // 形态过滤（严格模式）：只在平行震荡做T
    //
    // 注意一个内在矛盾：股价急跌至布林下轨时，布林带本身往往正处「开口向下」。
    // 若照搬第一步「开口向下不进场」去否决正T，会**系统性错过全部低吸机会**——
    // 因为低吸信号恰恰诞生于急跌末端。
    // 因此默认模式下形态只作为信号强度的加分项，不否决方向；
    // 只有显式开启 trend_filter 时才作为硬条件（信号稀少但质量更高）。
    if config.trend_filter && shape != BollShape::Flat {
        return None;
    }

    // 缩量确认
    if config.volume_filter && !volume_shrunk {
        return None;
    }

    // ── 正T：低吸（买入）──
    // 触发三要素：窗口内 J 曾超卖 + 窗口内曾触下轨 + 当日金叉确认
    if config.t_mode.allows_positive()
        && j_oversold_recent
        && touched_lower_recent
        && cross == KdjCross::Golden
    {
        let mut strength: f64 = 0.4;
        let mut reasons = vec![
            format!("J 值 {:.1}（近 {} 根曾跌破 {}，警戒已激活）", j, config.signal_window, config.j_oversold),
            "KDJ 金叉确认（收盘确认，避免盘中闪烁）".to_string(),
            format!(
                "近 {} 根内曾触及布林下轨 {:.2}（现价 {:.2} 已反弹）",
                config.signal_window, lower, price
            ),
        ];

        if volume_shrunk {
            strength += 0.2;
            reasons.push("缩量触轨，抛压衰竭".to_string());
        }
        if config.use_divergence && div == KdjDivergence::Bullish {
            strength += 0.2;
            reasons.push("KDJ 底背离，下跌动能衰竭（加分）".to_string());
        }
        if shape == BollShape::Flat {
            strength += 0.2;
            reasons.push("布林平行震荡，做T最佳环境".to_string());
        }

        return Some(TSignal {
            date: date.to_string(),
            index,
            direction: TDirection::Buy,
            action: SignalAction::Entry,
            position_pct: first_batch(config, strength),
            price,
            step: SignalStep::Triggered,
            t_type: "正T".to_string(),
            strength: strength.min(1.0),
            reasons,
            boll_shape: shape.label().to_string(),
            j_value: j,
            has_divergence: div == KdjDivergence::Bullish,
            volume_shrunk,
        });
    }

    // ── 反T：高抛（卖出）──
    // 触发三要素：窗口内 J 曾超买 + 窗口内曾触上轨 + 当日死叉确认
    if config.t_mode.allows_reverse()
        && j_overbought_recent
        && touched_upper_recent
        && cross == KdjCross::Dead
    {
        let mut strength: f64 = 0.4;
        let mut reasons = vec![
            format!("J 值 {:.1}（近 {} 根曾升破 {}，警戒已激活）", j, config.signal_window, config.j_overbought),
            "KDJ 死叉确认".to_string(),
            format!(
                "近 {} 根内曾触及布林上轨 {:.2}（现价 {:.2} 已回落）",
                config.signal_window, upper, price
            ),
        ];

        if volume_shrunk {
            strength += 0.2;
            reasons.push("缩量冲高，上攻乏力".to_string());
        }
        if config.use_divergence && div == KdjDivergence::Bearish {
            strength += 0.2;
            reasons.push("KDJ 顶背离，上涨动能衰竭（加分）".to_string());
        }
        if shape == BollShape::Flat {
            strength += 0.2;
            reasons.push("布林平行震荡，做T最佳环境".to_string());
        }

        return Some(TSignal {
            date: date.to_string(),
            index,
            direction: TDirection::Sell,
            action: SignalAction::Entry,
            position_pct: first_batch(config, strength),
            price,
            step: SignalStep::Triggered,
            t_type: "反T".to_string(),
            strength: strength.min(1.0),
            reasons,
            boll_shape: shape.label().to_string(),
            j_value: j,
            has_divergence: div == KdjDivergence::Bearish,
            volume_shrunk,
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use appcore::{Kline, TMode};

    /// 构造 K 线：close 由调用方给定，high/low 由 close 小幅扩展而来
    fn make_klines(closes: &[f64], base_vol: f64) -> Vec<Kline> {
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
                    volume: base_vol * (1.0 + 0.2 * (i as f64 * 1.7).sin()),
                    amount: 0.0,
                    pct_change: None,
                    turnover: None,
                }
            })
            .collect()
    }

    /// 构造一段「下跌 → 金叉反弹」的走势，用于验证正T信号
    fn downthen_up_series() -> Vec<f64> {
        let mut v = Vec::new();
        // 前 60 根横盘
        for i in 0..60 {
            v.push(10.0 + 0.1 * (i as f64 * 0.6).sin());
        }
        // 中间 12 根急跌（触发 J 超卖 + 触下轨）
        for i in 0..12 {
            v.push(10.0 - 0.25 * (i + 1) as f64);
        }
        // 后 20 根反弹（触发金叉）
        for i in 0..20 {
            v.push(7.0 + 0.12 * (i + 1) as f64);
        }
        v
    }

    #[test]
    fn test_signals_are_paired() {
        let kl = make_klines(&downthen_up_series(), 1_000_000.0);
        let mut cfg = TStrategyConfig::default();
        cfg.volume_filter = false; // 该构造的量能不满足缩量，先关掉
        cfg.trend_filter = false;
        let sigs = analyze_t_signals(&kl, &cfg);
        // 每两条信号构成一笔交易，且同一笔内方向相反（进场→出场）。
        //
        // 注意不能断言「相邻两条方向相反」：正T 是 买→卖、反T 是 卖→买，
        // 正T 的出场（卖）后面完全可以紧跟反T 的进场（卖），
        // 出现连续两个「卖出」是合法的。末尾允许留一条未平仓信号。
        for (i, pair) in sigs.chunks(2).enumerate() {
            if pair.len() == 2 {
                assert_ne!(
                    pair[0].direction, pair[1].direction,
                    "第 {} 笔交易的两个信号方向必须相反，实际序列：{:?}",
                    i + 1,
                    sigs.iter().map(|s| s.direction.label()).collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn test_no_signal_on_flat_series() {
        // 完全无波动的序列不应产生信号（RSV 恒为 0，无交叉）
        let kl = make_klines(&vec![10.0; 150], 1_000_000.0);
        let cfg = TStrategyConfig::default();
        let sigs = analyze_t_signals(&kl, &cfg);
        assert!(sigs.is_empty(), "无波动序列不应产生信号，实际 {} 条", sigs.len());
    }

    #[test]
    fn test_insufficient_data_returns_empty() {
        let kl = make_klines(&vec![10.0; 20], 1_000_000.0);
        let cfg = TStrategyConfig::default();
        assert!(analyze_t_signals(&kl, &cfg).is_empty());
    }

    #[test]
    fn test_positive_t_signal_has_expected_shape() {
        let kl = make_klines(&downthen_up_series(), 1_000_000.0);
        let mut cfg = TStrategyConfig::default();
        cfg.volume_filter = false;
        cfg.t_mode = TMode::Positive;
        let sigs = analyze_t_signals(&kl, &cfg);
        assert!(!sigs.is_empty(), "应产生正T信号");
        let first = &sigs[0];
        assert_eq!(first.direction, TDirection::Buy, "正T应先买入");
        assert_eq!(first.t_type, "正T");
        assert!(
            first.strength > 0.0 && first.strength <= 1.0,
            "信号强度应在 (0,1]，实际 {}",
            first.strength
        );
        assert!(!first.reasons.is_empty(), "应给出命中原因");
    }

    #[test]
    fn test_trend_filter_reduces_signals() {
        let kl = make_klines(&downthen_up_series(), 1_000_000.0);
        let mut loose = TStrategyConfig::default();
        loose.volume_filter = false;
        loose.trend_filter = false;
        let mut strict = TStrategyConfig::default();
        strict.volume_filter = false;
        strict.trend_filter = true;

        let a = analyze_t_signals(&kl, &loose).len();
        let b = analyze_t_signals(&kl, &strict).len();
        assert!(
            b <= a,
            "开启形态过滤后信号数不应增加：宽松={} 严格={}",
            a,
            b
        );
    }

    #[test]
    fn test_window_allows_delayed_cross() {
        // 核心回归：J 超卖与金叉不同日时仍应触发（时序状态机的意义所在）
        let kl = make_klines(&downthen_up_series(), 1_000_000.0);
        let mut narrow = TStrategyConfig::default();
        narrow.volume_filter = false;
        narrow.signal_window = 1; // 几乎要求同日 → 信号应显著减少或消失
        let mut wide = TStrategyConfig::default();
        wide.volume_filter = false;
        wide.signal_window = 10;

        let a = analyze_t_signals(&kl, &narrow).len();
        let b = analyze_t_signals(&kl, &wide).len();
        assert!(
            b >= a,
            "放宽警戒窗口后信号数不应减少：窗口1={} 窗口10={}",
            a,
            b
        );
    }
}
