//! 布林带形态识别
//!
//! 用于「做T六步法则」的第一步：确认大方向。
//! 把布林带状态量化为四类：
//! - OpeningUp   开口向上：趋势启动/加速，做T容易 T 飞（卖出后买不回）
//! - OpeningDown 开口向下：下跌趋势，KDJ 会钝化，J 值长期为负，抄底易套
//! - Flat        平行震荡：做T的最佳环境
//! - WeakTrend   弱趋势：介于震荡与明确趋势之间
//!
//! 判定依据两个维度：
//! 1. 带宽变化率：当前带宽 vs 近 N 日平均带宽（扩张 / 收缩）
//! 2. 中轨斜率：近 N 日中轨涨跌幅（%），反映趋势方向与力度

use serde::Serialize;

/// 布林带形态
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum BollShape {
    /// 开口向上：带宽扩张且中轨明显上行
    OpeningUp,
    /// 开口向下：带宽扩张且中轨明显下行
    OpeningDown,
    /// 平行震荡：带宽收缩或中轨走平 —— 最适合作T
    Flat,
    /// 弱趋势：有方向但力度不足，或未明确归类
    WeakTrend,
}

impl BollShape {
    pub fn label(&self) -> &'static str {
        match self {
            BollShape::OpeningUp => "开口向上",
            BollShape::OpeningDown => "开口向下",
            BollShape::Flat => "平行震荡",
            BollShape::WeakTrend => "弱趋势",
        }
    }

    /// 该形态下适合做T的程度：0=不适合，1=一般，2=适合
    pub fn t_suitability(&self) -> u8 {
        match self {
            BollShape::Flat => 2,
            BollShape::WeakTrend => 1,
            // 开口向上容易 T 飞，开口向下容易套牢，都不适合
            BollShape::OpeningUp => 0,
            BollShape::OpeningDown => 0,
        }
    }

    /// 该形态下允许的做T方向（true=允许）
    /// - 开口向上：只做正T（先买后卖），不做反T（先卖怕踏空）
    /// - 开口向下：只做反T（先卖后买），不做正T（先买怕接飞刀）
    /// - 震荡：双向均可
    pub fn allows(&self) -> (bool, bool) {
        match self {
            BollShape::OpeningUp => (true, false),
            BollShape::OpeningDown => (false, true),
            BollShape::Flat => (true, true),
            BollShape::WeakTrend => (true, true),
        }
    }
}

/// 单根 K 线的布林带形态结果
#[derive(Debug, Clone, Serialize)]
pub struct BollShapeResult {
    pub date: String,
    /// 形态分类
    pub shape: BollShape,
    /// 带宽 =（上轨-下轨）/ 中轨 * 100
    pub bandwidth: f64,
    /// 当前带宽在过去 lookback 根中的分位数（0~100，越小表示越"窄"）
    pub bandwidth_pct: f64,
    /// 中轨近 slope_n 根涨跌幅（%）
    pub mid_slope: f64,
    /// 形态的中文解读
    pub note: String,
}

/// 判定布林带形态
///
/// - `boll`：calc_boll 的结果（长度与 dates 对齐，前面不足 n-1 的位置由调用方保证不传入）
/// - `dates`：日期序列
/// - `slope_n`：中轨斜率回看根数（默认 20）
/// - `slope_threshold`：判定"明显上行/下行"的斜率阈值（%，默认 1.5）
/// - `flat_slope`：判定"走平"的斜率阈值（%，默认 1.0）
/// - `expand_ratio`：带宽扩张判定倍数（默认 1.10）
/// - `contract_ratio`：带宽收缩判定倍数（默认 0.85）
/// - `pct_lookback`：带宽分位回看根数（默认 250）
#[allow(clippy::too_many_arguments)]
pub fn calc_boll_shape(
    boll: &[crate::bollinger::BollResult],
    slope_n: usize,
    slope_threshold: f64,
    flat_slope: f64,
    expand_ratio: f64,
    contract_ratio: f64,
    pct_lookback: usize,
) -> Vec<BollShapeResult> {
    let len = boll.len();
    if len == 0 {
        return Vec::new();
    }

    let bw_n = 20; // 带宽均值窗口
    let mut out = Vec::with_capacity(len);

    for i in 0..len {
        let cur = &boll[i];
        let bw = (cur.upper - cur.lower) / cur.mid * 100.0;

        // 中轨斜率
        let slope = if i >= slope_n {
            let prev = boll[i - slope_n].mid;
            if prev > 0.0 {
                (cur.mid - prev) / prev * 100.0
            } else {
                0.0
            }
        } else {
            0.0
        };

        // 带宽均值（近 bw_n 根）
        let start = i.saturating_sub(bw_n - 1);
        let bw_avg = boll[start..=i]
            .iter()
            .map(|b| (b.upper - b.lower) / b.mid * 100.0)
            .sum::<f64>()
            / (i - start + 1) as f64;

        // 带宽分位
        let pstart = i.saturating_sub(pct_lookback - 1);
        let window: Vec<f64> = boll[pstart..=i]
            .iter()
            .map(|b| (b.upper - b.lower) / b.mid * 100.0)
            .collect();
        let bandwidth_pct = if window.is_empty() {
            50.0
        } else {
            let below = window.iter().filter(|&&x| x <= bw).count();
            below as f64 / window.len() as f64 * 100.0
        };

        let expanding = bw > bw_avg * expand_ratio;
        let contracting = bw < bw_avg * contract_ratio;

        let shape = if expanding && slope > slope_threshold {
            BollShape::OpeningUp
        } else if expanding && slope < -slope_threshold {
            BollShape::OpeningDown
        } else if contracting || slope.abs() < flat_slope {
            BollShape::Flat
        } else {
            BollShape::WeakTrend
        };

        let note = match shape {
            BollShape::OpeningUp => format!(
                "带宽扩张、中轨上行{:.2}%，趋势启动；做T易「T飞」，建议只做正T或观望",
                slope
            ),
            BollShape::OpeningDown => format!(
                "带宽扩张、中轨下行{:.2}%，下跌趋势；KDJ 易钝化，建议只做反T或观望",
                slope
            ),
            BollShape::Flat => format!(
                "带宽分位{:.0}%、中轨斜率{:.2}%，震荡格局，做T最佳环境",
                bandwidth_pct, slope
            ),
            BollShape::WeakTrend => format!(
                "带宽分位{:.0}%、中轨斜率{:.2}%，方向未明，做T需缩小仓位",
                bandwidth_pct, slope
            ),
        };

        out.push(BollShapeResult {
            date: cur.date.clone(),
            shape,
            bandwidth: bw,
            bandwidth_pct,
            mid_slope: slope,
            note,
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bollinger::calc_boll;

    #[test]
    fn test_flat_market_is_best_for_t() {
        // 横盘：价格在 10 附近小幅震荡
        let n = 120;
        let closes: Vec<f64> = (0..n)
            .map(|i| 10.0 + 0.15 * (i as f64 * 0.5).sin())
            .collect();
        let dates: Vec<String> = (0..n).map(|i| format!("d{:03}", i)).collect();
        let boll = calc_boll(&closes, &dates, 20, 2.0);
        let shapes = calc_boll_shape(&boll, 20, 1.5, 1.0, 1.10, 0.85, 250);
        let last = shapes.last().unwrap();
        assert_eq!(last.shape, BollShape::Flat, "横盘应判为平行震荡");
        assert_eq!(last.shape.t_suitability(), 2);
        assert_eq!(last.shape.allows(), (true, true), "震荡市双向可做");
    }

    /// 构造「横盘收口 → 放量突破」的经典布林开口形态：
    /// 前 100 根窄幅横盘（带宽约 2%），后 12 根方向性突破且日波动放大（带宽约 32%）。
    ///
    /// 注意两点（都是构造这类测试时的真实陷阱）：
    /// 1. 不能用**线性**上涨序列测试「开口向上」——线性序列标准差恒定、中轨同步上移，
    ///    带宽比值反而不会扩大，会被判为 Flat。
    /// 2. 突破段不能太长：带宽均值窗口只有 20 根，突破持续超过 20 根后
    ///    均值会「追上」高波动，导致检测不到扩张。所以突破段取 12 根。
    fn squeeze_then_breakout(up: bool) -> (Vec<f64>, Vec<String>) {
        let n = 112;
        let flat = 100usize;
        let dir = if up { 1.0 } else { -1.0 };
        let mut closes = Vec::with_capacity(n);
        for i in 0..n {
            if i < flat {
                // 横盘：窄幅震荡
                closes.push(10.0 + 0.06 * (i as f64 * 0.7).sin());
            } else {
                let s = (i - flat + 1) as f64;
                let drift = dir * 10.0 * 0.25 * (s / 12.0); // 12 根内运行 25%
                let osc = (0.15 + 0.25 * (s / 12.0)) * (s * 1.1).sin(); // 日波动放大
                closes.push(10.0 + drift + osc);
            }
        }
        let dates: Vec<String> = (0..n).map(|i| format!("d{:03}", i)).collect();
        (closes, dates)
    }

    #[test]
    fn test_breakout_up_is_opening_up() {
        let (closes, dates) = squeeze_then_breakout(true);
        let boll = calc_boll(&closes, &dates, 20, 2.0);
        let shapes = calc_boll_shape(&boll, 20, 1.5, 1.0, 1.10, 0.85, 250);
        let last = shapes.last().unwrap();
        assert_eq!(last.shape, BollShape::OpeningUp, "收口后向上突破应判为开口向上");
        assert!(last.mid_slope > 0.0, "中轨应上行");
        assert_eq!(last.shape.t_suitability(), 0, "开口向上不适合做T（易T飞）");
        assert_eq!(last.shape.allows(), (true, false), "只做正T，不做反T");
    }

    #[test]
    fn test_breakout_down_is_opening_down() {
        let (closes, dates) = squeeze_then_breakout(false);
        let boll = calc_boll(&closes, &dates, 20, 2.0);
        let shapes = calc_boll_shape(&boll, 20, 1.5, 1.0, 1.10, 0.85, 250);
        let last = shapes.last().unwrap();
        assert_eq!(last.shape, BollShape::OpeningDown, "收口后向下破位应判为开口向下");
        assert!(last.mid_slope < 0.0, "中轨应下行");
        assert_eq!(last.shape.allows(), (false, true), "只做反T，不做正T");
    }
}
