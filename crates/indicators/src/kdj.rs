use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct KdjResult {
    pub date: String,
    pub k: f64,
    pub d: f64,
    pub j: f64,
}

/// 计算 KDJ 指标 (n=9, m1=3, m2=3)
pub fn calc_kdj(highs: &[f64], lows: &[f64], closes: &[f64], dates: &[String], n: usize) -> Vec<KdjResult> {
    if closes.len() < n {
        return Vec::new();
    }

    let mut k = 50.0_f64;
    let mut d = 50.0_f64;
    let mut results = Vec::new();

    for i in 0..closes.len() {
        if i < n - 1 {
            continue;
        }

        let mut hh = f64::MIN;
        let mut ll = f64::MAX;
        for j in (i + 1 - n)..=i {
            hh = hh.max(highs[j]);
            ll = ll.min(lows[j]);
        }

        let rsv = if hh == ll {
            0.0
        } else {
            (closes[i] - ll) / (hh - ll) * 100.0
        };

        k = (2.0 / 3.0) * k + (1.0 / 3.0) * rsv;
        d = (2.0 / 3.0) * d + (1.0 / 3.0) * k;
        let j = 3.0 * k - 2.0 * d;

        if let Some(date) = dates.get(i) {
            results.push(KdjResult {
                date: date.clone(),
                k,
                d,
                j,
            });
        }
    }

    results
}

/// KDJ 交叉类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum KdjCross {
    /// 金叉：K 上穿 D
    Golden,
    /// 死叉：K 下穿 D
    Dead,
    /// 无交叉
    None,
}

impl KdjCross {
    pub fn label(&self) -> &'static str {
        match self {
            KdjCross::Golden => "金叉",
            KdjCross::Dead => "死叉",
            KdjCross::None => "无",
        }
    }
}

/// 检测每根 K 线的 KDJ 交叉状态
///
/// 返回长度与 `kdj` 相同，第 0 根恒为 `None`（无前值可比较）。
/// 用 **收盘确认**：只有当根与前一根的 K/D 关系发生反转才算交叉，避免信号闪烁。
pub fn detect_cross(kdj: &[KdjResult]) -> Vec<KdjCross> {
    if kdj.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(kdj.len());
    out.push(KdjCross::None);
    for i in 1..kdj.len() {
        let prev = &kdj[i - 1];
        let cur = &kdj[i];
        let cross = if prev.k <= prev.d && cur.k > cur.d {
            KdjCross::Golden
        } else if prev.k >= prev.d && cur.k < cur.d {
            KdjCross::Dead
        } else {
            KdjCross::None
        };
        out.push(cross);
    }
    out
}

/// KDJ 背离类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum KdjDivergence {
    /// 底背离：价格创新低，但 J 值低点抬高 —— 下跌动能衰竭
    Bullish,
    /// 顶背离：价格创新高，但 J 值高点降低 —— 上涨动能衰竭
    Bearish,
    /// 无背离
    None,
}

impl KdjDivergence {
    pub fn label(&self) -> &'static str {
        match self {
            KdjDivergence::Bullish => "底背离",
            KdjDivergence::Bearish => "顶背离",
            KdjDivergence::None => "无背离",
        }
    }
}

/// 检测 KDJ 背离
///
/// - 底背离：当前最低价 ≤ 窗口内前低（创新低），但当前 J > 前低处的 J
/// - 顶背离：当前最高价 ≥ 窗口内前高（创新高），但当前 J < 前高处的 J
///
/// `lookback` 回看根数；`tol` 创新高/新低的容差比例（0.0 为严格比较）。
pub fn detect_divergence(
    kdj: &[KdjResult],
    lows: &[f64],
    highs: &[f64],
    lookback: usize,
    tol: f64,
) -> Vec<KdjDivergence> {
    let n = kdj.len();
    if n == 0 || lows.len() != n || highs.len() != n {
        return vec![KdjDivergence::None; n];
    }

    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        if i < lookback {
            out.push(KdjDivergence::None);
            continue;
        }
        let start = i - lookback;
        let mut lo_pos = start;
        let mut hi_pos = start;
        for j in start..i {
            if lows[j] < lows[lo_pos] {
                lo_pos = j;
            }
            if highs[j] > highs[hi_pos] {
                hi_pos = j;
            }
        }

        let cur_j = kdj[i].j;
        let new_low = lows[i] <= lows[lo_pos] * (1.0 + tol);
        let new_high = highs[i] >= highs[hi_pos] * (1.0 - tol);

        if new_low && cur_j > kdj[lo_pos].j {
            out.push(KdjDivergence::Bullish);
        } else if new_high && cur_j < kdj[hi_pos].j {
            out.push(KdjDivergence::Bearish);
        } else {
            out.push(KdjDivergence::None);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_cross_golden_and_dead() {
        let kdj = vec![
            KdjResult { date: "d1".into(), k: 20.0, d: 30.0, j: 0.0 },
            KdjResult { date: "d2".into(), k: 35.0, d: 30.0, j: 45.0 },
            KdjResult { date: "d3".into(), k: 40.0, d: 30.0, j: 60.0 },
            KdjResult { date: "d4".into(), k: 25.0, d: 30.0, j: 15.0 },
        ];
        let c = detect_cross(&kdj);
        assert_eq!(c[0], KdjCross::None);
        assert_eq!(c[1], KdjCross::Golden);
        assert_eq!(c[2], KdjCross::None);
        assert_eq!(c[3], KdjCross::Dead);
    }

    #[test]
    fn test_detect_cross_empty() {
        assert!(detect_cross(&[]).is_empty());
    }

    #[test]
    fn test_bullish_divergence() {
        let n = 40;
        let kdj: Vec<KdjResult> = (0..n)
            .map(|i| KdjResult {
                date: format!("d{:02}", i),
                k: 30.0,
                d: 30.0,
                j: if i == n - 1 { 20.0 } else { -10.0 },
            })
            .collect();
        let lows: Vec<f64> = (0..n).map(|i| if i == n - 1 { 4.5 } else { 5.0 }).collect();
        let highs: Vec<f64> = vec![10.0; n];
        let div = detect_divergence(&kdj, &lows, &highs, 30, 0.0);
        assert_eq!(div[n - 1], KdjDivergence::Bullish, "价创新低但J抬高=底背离");
    }

    #[test]
    fn test_bearish_divergence() {
        let n = 40;
        let kdj: Vec<KdjResult> = (0..n)
            .map(|i| KdjResult {
                date: format!("d{:02}", i),
                k: 70.0,
                d: 70.0,
                j: if i == n - 1 { 80.0 } else { 110.0 },
            })
            .collect();
        let lows: Vec<f64> = vec![5.0; n];
        let highs: Vec<f64> = (0..n)
            .map(|i| if i == n - 1 { 10.5 } else { 10.0 })
            .collect();
        let div = detect_divergence(&kdj, &lows, &highs, 30, 0.0);
        assert_eq!(div[n - 1], KdjDivergence::Bearish, "价创新高但J降低=顶背离");
    }

    #[test]
    fn test_no_divergence_when_aligned() {
        let n = 40;
        let kdj: Vec<KdjResult> = (0..n)
            .map(|i| KdjResult { date: format!("d{:02}", i), k: 50.0, d: 50.0, j: 50.0 })
            .collect();
        let lows: Vec<f64> = vec![5.0; n];
        let highs: Vec<f64> = vec![10.0; n];
        let div = detect_divergence(&kdj, &lows, &highs, 30, 0.0);
        assert_eq!(div[n - 1], KdjDivergence::None);
    }
}
