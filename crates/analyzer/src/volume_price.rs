//! 量价分析模块
//!
//! 把《量价分析：盘口解读法》的核心方法论落地为可计算的信号：
//! 1. 单根 K 线的量价关系分类（量增价升 / 量增价跌 / 量缩价升 / 量缩价跌 / 价平量平…）
//! 2. 成交量相对均量（MA5 / MA10）的位置，判断放量 / 缩量 / 平量
//! 3. 量价背离（顶背离、底背离）
//! 4. 底部放量止跌（长期下跌后首根放量长阳 / 长下影）
//! 5. 放量突破 / 破位（价格突破近期高低点且成交量配合）
//! 最终给出量价 verdict 与中文结论。

use serde::Serialize;
use appcore::Kline;

/// 量价关系分类
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum VpRelationship {
    /// 量增价升：健康上涨
    VolumeUpPriceUp,
    /// 量增价跌：恐慌抛售 / 主力出逃
    VolumeUpPriceDown,
    /// 量平价升：上涨动能一般
    VolumeFlatPriceUp,
    /// 量平价跌：弱势下跌
    VolumeFlatPriceDown,
    /// 量缩价升：上涨无量，动能不足
    VolumeDownPriceUp,
    /// 量缩价跌：抛压减轻
    VolumeDownPriceDown,
    /// 价平量平：观望
    Neutral,
}

impl VpRelationship {
    pub fn label(&self) -> &'static str {
        match self {
            VpRelationship::VolumeUpPriceUp => "量增价升",
            VpRelationship::VolumeUpPriceDown => "量增价跌",
            VpRelationship::VolumeFlatPriceUp => "价升量平",
            VpRelationship::VolumeFlatPriceDown => "价跌量平",
            VpRelationship::VolumeDownPriceUp => "量缩价升",
            VpRelationship::VolumeDownPriceDown => "量缩价跌",
            VpRelationship::Neutral => "价平量平",
        }
    }

    /// 偏多 / 偏空 / 中性
    pub fn bias(&self) -> i8 {
        match self {
            VpRelationship::VolumeUpPriceUp => 1,
            VpRelationship::VolumeDownPriceDown => 1,
            VpRelationship::VolumeUpPriceDown => -1,
            VpRelationship::VolumeDownPriceUp => -1,
            _ => 0,
        }
    }
}

/// 单根 K 线的量价关系记录
#[derive(Debug, Clone, Serialize)]
pub struct VpBar {
    pub date: String,
    pub close: f64,
    pub volume: f64,
    pub amount: f64,
    pub pct_change: f64,
    /// 成交量相对 MA5 的倍数
    pub volume_ratio: f64,
    pub relationship: VpRelationship,
    /// 该根 K 线的量价解读
    pub note: String,
}

/// 量价信号类型
#[derive(Debug, Clone, Serialize)]
pub enum VpSignalType {
    Buy,
    Sell,
    Watch,
}

/// 量价分析产生的关键信号
#[derive(Debug, Clone, Serialize)]
pub struct VpSignal {
    pub signal_type: VpSignalType,
    pub name: String,
    pub date: String,
    pub strength: String,
    pub description: String,
}

/// 量价分析完整结果
#[derive(Debug, Clone, Serialize)]
pub struct VolumePriceAnalysis {
    /// 一句话总结（当前量价状态）
    pub summary: String,
    /// 量价 verdict：看多 / 看空 / 中性
    pub verdict: String,
    /// 放量 / 缩量 / 平量
    pub volume_status: String,
    /// 当前成交量相对 MA5 的倍数
    pub current_volume_ratio: f64,
    /// 最近 N 根 K 线的量价关系（用于前端展示）
    pub recent_bars: Vec<VpBar>,
    /// 关键量价信号（背离 / 底部放量 / 突破等）
    pub signals: Vec<VpSignal>,
    /// 中文结论
    pub conclusion: String,
}

fn sma(values: &[f64], i: usize, window: usize) -> f64 {
    if i + 1 < window {
        return 0.0;
    }
    let start = i + 1 - window;
    let sum: f64 = values[start..=i].iter().sum();
    sum / window as f64
}

/// 量价分析主函数
pub fn analyze_volume_price(klines: &[Kline]) -> VolumePriceAnalysis {
    if klines.len() < 10 {
        return VolumePriceAnalysis {
            summary: "K线数据不足，无法进行量价分析".to_string(),
            verdict: "中性".to_string(),
            volume_status: "未知".to_string(),
            current_volume_ratio: 0.0,
            recent_bars: vec![],
            signals: vec![],
            conclusion: "至少需要 10 根 K 线数据才能完成量价分析。".to_string(),
        };
    }

    let n = klines.len();
    let closes: Vec<f64> = klines.iter().map(|k| k.close).collect();
    let volumes: Vec<f64> = klines.iter().map(|k| k.volume).collect();
    let dates: Vec<String> = klines.iter().map(|k| k.date.clone()).collect();

    // 计算每根 K 线的量价关系
    let mut bars: Vec<VpBar> = Vec::with_capacity(n);
    for i in 0..n {
        let ma5 = sma(&volumes, i, 5);
        let vol_ratio = if ma5 > 0.0 { volumes[i] / ma5 } else { 1.0 };
        let pct = klines[i].pct_change.unwrap_or(0.0);

        let relationship = classify(vol_ratio, pct);
        let note = relationship_note(relationship, pct);

        bars.push(VpBar {
            date: dates[i].clone(),
            close: closes[i],
            volume: volumes[i],
            amount: klines[i].amount,
            pct_change: pct,
            volume_ratio: vol_ratio,
            relationship,
            note,
        });
    }

    let current_ratio = bars.last().map(|b| b.volume_ratio).unwrap_or(1.0);
    let volume_status = if current_ratio > 1.2 {
        "放量"
    } else if current_ratio < 0.8 {
        "缩量"
    } else {
        "平量"
    }
    .to_string();

    // 取最近 24 根用于展示
    let recent_bars: Vec<VpBar> = bars.iter().rev().take(24).cloned().collect::<Vec<_>>().into_iter().rev().collect();

    // ===== 关键信号检测 =====
    let mut signals: Vec<VpSignal> = Vec::new();

    // 1. 量价顶背离：近期收盘价接近 20 日新高，但成交量较前期萎缩
    detect_divergence(&bars, &closes, &volumes, &dates, &mut signals);

    // 2. 底部放量止跌：长期下跌后首根放量阳线（长下影）
    detect_bottom_reversal(&klines, &bars, &mut signals);

    // 3. 放量突破 / 破位：价格突破近期高低点且成交量配合
    detect_breakthrough(&closes, &volumes, &dates, &bars, &mut signals);

    // 4. 当前量价状态信号（最近一根）
    if let Some(last) = bars.last() {
        match last.relationship {
            VpRelationship::VolumeUpPriceUp => {
                signals.push(VpSignal {
                    signal_type: VpSignalType::Buy,
                    name: "放量上涨".to_string(),
                    date: last.date.clone(),
                    strength: "中".to_string(),
                    description: format!(
                        "当前放量上涨（量比 {:.2}），量价配合健康，趋势有延续性",
                        last.volume_ratio
                    ),
                });
            }
            VpRelationship::VolumeUpPriceDown => {
                signals.push(VpSignal {
                    signal_type: VpSignalType::Sell,
                    name: "放量下跌".to_string(),
                    date: last.date.clone(),
                    strength: "中".to_string(),
                    description: format!(
                        "当前放量下跌（量比 {:.2}），抛压沉重，警惕继续下行",
                        last.volume_ratio
                    ),
                });
            }
            VpRelationship::VolumeDownPriceUp => {
                signals.push(VpSignal {
                    signal_type: VpSignalType::Watch,
                    name: "量缩价升".to_string(),
                    date: last.date.clone(),
                    strength: "弱".to_string(),
                    description: format!(
                        "当前缩量上涨（量比 {:.2}），上涨无量，动能不足，注意回落",
                        last.volume_ratio
                    ),
                });
            }
            VpRelationship::VolumeDownPriceDown => {
                signals.push(VpSignal {
                    signal_type: VpSignalType::Watch,
                    name: "量缩价跌".to_string(),
                    date: last.date.clone(),
                    strength: "弱".to_string(),
                    description: format!(
                        "当前缩量下跌（量比 {:.2}），抛压减轻，可能接近阶段底部",
                        last.volume_ratio
                    ),
                });
            }
            _ => {}
        }
    }

    // ===== 综合 verdict =====
    let mut score: i32 = 0;
    for s in &signals {
        let w = match s.strength.as_str() {
            "强" => 2,
            "中" => 1,
            _ => 0,
        };
        match s.signal_type {
            VpSignalType::Buy => score += w,
            VpSignalType::Sell => score -= w,
            VpSignalType::Watch => {}
        }
    }
    // 最近 5 根量价关系偏置
    for b in bars.iter().rev().take(5) {
        score += b.relationship.bias() as i32;
    }

    let verdict = if score >= 2 {
        "看多"
    } else if score <= -2 {
        "看空"
    } else {
        "中性"
    }
    .to_string();

    // ===== 中文结论 =====
    let summary = format!(
        "当前处于「{}」状态（量比 {:.2}），量价 verdict：{}",
        volume_status, current_ratio, verdict
    );
    let conclusion = build_conclusion(&volume_status, current_ratio, &signals, &verdict);

    VolumePriceAnalysis {
        summary,
        verdict,
        volume_status,
        current_volume_ratio: current_ratio,
        recent_bars,
        signals,
        conclusion,
    }
}

/// 根据量比和涨跌幅分类量价关系
fn classify(vol_ratio: f64, pct: f64) -> VpRelationship {
    let up = pct > 0.3;
    let down = pct < -0.3;
    let vol_up = vol_ratio > 1.2;
    let vol_down = vol_ratio < 0.8;

    match (vol_up, vol_down, up, down) {
        (true, _, true, _) => VpRelationship::VolumeUpPriceUp,
        (true, _, _, true) => VpRelationship::VolumeUpPriceDown,
        (_, true, true, _) => VpRelationship::VolumeDownPriceUp,
        (_, true, _, true) => VpRelationship::VolumeDownPriceDown,
        (_, _, true, _) => VpRelationship::VolumeFlatPriceUp,
        (_, _, _, true) => VpRelationship::VolumeFlatPriceDown,
        _ => VpRelationship::Neutral,
    }
}

fn relationship_note(r: VpRelationship, pct: f64) -> String {
    match r {
        VpRelationship::VolumeUpPriceUp => format!("放量上涨 {:.2}%，量价配合", pct),
        VpRelationship::VolumeUpPriceDown => format!("放量下跌 {:.2}%，抛压重", pct),
        VpRelationship::VolumeFlatPriceUp => format!("价升量平 {:.2}%，动能一般", pct),
        VpRelationship::VolumeFlatPriceDown => format!("价跌量平 {:.2}%，弱势", pct),
        VpRelationship::VolumeDownPriceUp => format!("缩量上涨 {:.2}%，无量推升", pct),
        VpRelationship::VolumeDownPriceDown => format!("缩量下跌 {:.2}%，抛压减轻", pct),
        VpRelationship::Neutral => format!("窄幅震荡 {:.2}%", pct),
    }
}

/// 量价背离检测（顶背离 / 底背离）
fn detect_divergence(
    bars: &[VpBar],
    closes: &[f64],
    volumes: &[f64],
    dates: &[String],
    signals: &mut Vec<VpSignal>,
) {
    let n = bars.len();
    if n < 25 {
        return;
    }
    let window = 20usize;

    // 顶部背离：最近一根接近区间新高，但近 5 日均量 < 前 5 日均量（量能跟不上）
    let recent_high = closes[n - window..n - 1].iter().cloned().fold(0.0_f64, f64::max);
    let last_close = closes[n - 1];
    let near_high = last_close >= recent_high * 0.99 && last_close <= recent_high * 1.01;
    let avg_vol_recent: f64 = volumes[n - 5..].iter().sum::<f64>() / 5.0;
    let avg_vol_prev: f64 = volumes[n - 10..n - 5].iter().sum::<f64>() / 5.0;
    if near_high && avg_vol_recent < avg_vol_prev * 0.85 {
        signals.push(VpSignal {
            signal_type: VpSignalType::Sell,
            name: "量价顶背离".to_string(),
            date: dates[n - 1].clone(),
            strength: "强".to_string(),
            description: format!(
                "价格接近 {} 日新高但近 5 日均量较前 5 日萎缩 {:.0}%，上涨动能枯竭，警惕反转",
                window,
                (1.0 - avg_vol_recent / avg_vol_prev) * 100.0
            ),
        });
    }

    // 底部背离：最近一根接近区间新低，但成交量未同步放大（抛压衰竭）
    let recent_low = closes[n - window..n - 1].iter().cloned().fold(f64::MAX, f64::min);
    let near_low = last_close <= recent_low * 1.01 && last_close >= recent_low * 0.99;
    if near_low && avg_vol_recent <= avg_vol_prev * 1.0 {
        signals.push(VpSignal {
            signal_type: VpSignalType::Buy,
            name: "量价底背离".to_string(),
            date: dates[n - 1].clone(),
            strength: "中".to_string(),
            description: format!(
                "价格接近 {} 日新低但量能未明显放大（近 5 日均量较前 5 日 {}），抛压衰竭，存在企稳可能",
                window,
                if avg_vol_recent < avg_vol_prev { "缩小" } else { "走平" }
            ),
        });
    }
}

/// 底部放量止跌：长期下跌后首根放量阳线（长下影）
fn detect_bottom_reversal(
    klines: &[Kline],
    bars: &[VpBar],
    signals: &mut Vec<VpSignal>,
) {
    let n = bars.len();
    if n < 15 {
        return;
    }
    // 取前 10 根看是否处于下跌趋势
    let mut downtrend = true;
    for i in (n - 11)..(n - 1) {
        if klines[i].close > klines[i - 1].close {
            downtrend = false;
            break;
        }
    }
    // 最近一根：放量（量比>1.5）且收阳，且下影线较长
    let last = &klines[n - 1];
    let bar = &bars[n - 1];
    let body_low = last.open.min(last.close);
    let lower_shadow = body_low - last.low;
    let body = (last.close - last.open).abs();
    let is_yang = last.close > last.open;
    let long_lower = lower_shadow > body * 1.5 && lower_shadow > 0.0;

    if downtrend && bar.volume_ratio > 1.5 && is_yang {
        signals.push(VpSignal {
            signal_type: VpSignalType::Buy,
            name: "底部放量止跌".to_string(),
            date: bar.date.clone(),
            strength: "强".to_string(),
            description: format!(
                "经历下跌趋势后首根放量阳线（量比 {:.2}）{}，显示低位有资金承接，止跌信号",
                bar.volume_ratio,
                if long_lower { "，且带长下影" } else { "" }
            ),
        });
    }
}

/// 放量突破 / 破位
fn detect_breakthrough(
    closes: &[f64],
    volumes: &[f64],
    dates: &[String],
    bars: &[VpBar],
    signals: &mut Vec<VpSignal>,
) {
    let n = bars.len();
    if n < 21 {
        return;
    }
    let window = 20usize;
    let recent_high = closes[n - window..n - 1].iter().cloned().fold(0.0_f64, f64::max);
    let recent_low = closes[n - window..n - 1].iter().cloned().fold(f64::MAX, f64::min);
    let last_close = closes[n - 1];
    let ma5 = sma(volumes, n - 1, 5);
    let vol_ratio = if ma5 > 0.0 { volumes[n - 1] / ma5 } else { 1.0 };

    if last_close > recent_high && vol_ratio > 1.5 {
        signals.push(VpSignal {
            signal_type: VpSignalType::Buy,
            name: "放量突破".to_string(),
            date: dates[n - 1].clone(),
            strength: "强".to_string(),
            description: format!(
                "价格放量突破 {} 日高点 {:.2}（量比 {:.2}），突破有效，可看高一线",
                window, recent_high, vol_ratio
            ),
        });
    } else if last_close > recent_high {
        signals.push(VpSignal {
            signal_type: VpSignalType::Watch,
            name: "无量突破".to_string(),
            date: dates[n - 1].clone(),
            strength: "弱".to_string(),
            description: format!(
                "价格突破 {} 日高点但量比仅 {:.2}，无量配合，警惕假突破",
                window, vol_ratio
            ),
        });
    }

    if last_close < recent_low && vol_ratio > 1.5 {
        signals.push(VpSignal {
            signal_type: VpSignalType::Sell,
            name: "放量破位".to_string(),
            date: dates[n - 1].clone(),
            strength: "强".to_string(),
            description: format!(
                "价格放量跌破 {} 日低点 {:.2}（量比 {:.2}），破位确认，回避风险",
                window, recent_low, vol_ratio
            ),
        });
    }
}

fn build_conclusion(
    volume_status: &str,
    ratio: f64,
    signals: &[VpSignal],
    verdict: &str,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(format!(
        "当前成交量处于「{}」状态（量比 {:.2}），量价 verdict 为「{}」。",
        volume_status, ratio, verdict
    ));

    if signals.is_empty() {
        parts.push("暂未检测到显著的量价异动信号，建议结合趋势与指标综合判断。".to_string());
    } else {
        let mut buy = 0;
        let mut sell = 0;
        for s in signals {
            match s.signal_type {
                VpSignalType::Buy => buy += 1,
                VpSignalType::Sell => sell += 1,
                VpSignalType::Watch => {}
            }
        }
        if buy > 0 {
            parts.push(format!(
                "偏多信号 {} 个（如放量上涨、底部放量止跌、放量突破、底背离），说明有资金主动介入。",
                buy
            ));
        }
        if sell > 0 {
            parts.push(format!(
                "偏空信号 {} 个（如放量下跌、顶背离、放量破位），提示抛压或加重或动能衰竭。",
                sell
            ));
        }
        // 列举最强信号
        if let Some(s) = signals.iter().find(|s| s.strength == "强") {
            parts.push(format!("关键信号：{} —— {}", s.name, s.description));
        }
    }

    parts.push(
        "量价是验证价格运动的标尺：放量上涨与缩量回调属健康结构，缩量上涨与放量下跌需警惕。\
         本分析仅基于历史量价数据，不构成投资建议。"
            .to_string(),
    );

    parts.join("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use appcore::Kline;

    fn mk(date: &str, open: f64, close: f64, high: f64, low: f64, vol: f64, pct: f64) -> Kline {
        Kline {
            stock_code: "TEST".into(),
            date: date.into(),
            open,
            close,
            high,
            low,
            volume: vol,
            amount: vol,
            pct_change: Some(pct),
            turnover: Some(1.0),
        }
    }

    #[test]
    fn test_volume_up_price_up_classification() {
        // 21 根平量小阳 + 最后一根放量长阳突破
        let mut k: Vec<Kline> = Vec::new();
        for i in 0..21 {
            k.push(mk(&format!("2026-01-{:02}", i + 1), 10.0, 10.0 + i as f64 * 0.1, 10.2, 9.9, 1000.0, 0.5));
        }
        // 最后一根放量突破（价 > 前高，量比>1.5）
        let last = k.len() - 1;
        k[last] = mk("2026-01-22", 10.9, 12.5, 12.6, 10.9, 3000.0, 9.0);

        let a = analyze_volume_price(&k);
        assert!(!a.signals.is_empty(), "应检测到突破信号");
        let has_break = a.signals.iter().any(|s| s.name == "放量突破");
        assert!(has_break, "应检测到放量突破");
        assert_eq!(a.verdict, "看多");
        assert_eq!(a.volume_status, "放量");
    }

    #[test]
    fn test_top_divergence() {
        // 价格创新高但量能萎缩 -> 顶背离
        let mut k: Vec<Kline> = Vec::new();
        for i in 0..25 {
            k.push(mk(&format!("2026-02-{:02}", i + 1), 10.0, 10.0 + i as f64 * 0.2, 10.3, 9.9, 1000.0, 1.0));
        }
        // 最后 5 根维持高位但缩量
        for i in 20..25 {
            k[i] = mk(&format!("2026-02-{:02}", i + 1), 14.8, 15.0, 15.1, 14.7, 500.0, 0.2);
        }
        let a = analyze_volume_price(&k);
        let has_div = a.signals.iter().any(|s| s.name == "量价顶背离");
        assert!(has_div, "应检测到量价顶背离");
        assert_eq!(a.verdict, "看空");
    }

    #[test]
    fn test_insufficient_data() {
        let k = vec![mk("2026-03-01", 10.0, 10.0, 10.0, 10.0, 100.0, 0.0)];
        let a = analyze_volume_price(&k);
        assert_eq!(a.verdict, "中性");
        assert!(a.recent_bars.is_empty());
    }
}
