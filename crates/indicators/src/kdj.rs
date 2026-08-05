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
