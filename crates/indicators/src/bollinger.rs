use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct BollResult {
    pub date: String,
    pub upper: f64,
    pub mid: f64,
    pub lower: f64,
}

/// 计算布林带 (n=20, k=2)
pub fn calc_boll(closes: &[f64], dates: &[String], n: usize, k: f64) -> Vec<BollResult> {
    if closes.len() < n {
        return Vec::new();
    }

    (0..closes.len()).filter_map(|i| {
        if i < n - 1 {
            return None;
        }

        let window = &closes[i + 1 - n..=i];
        let mid = window.iter().sum::<f64>() / n as f64;
        let variance = window.iter().map(|x| (x - mid).powi(2)).sum::<f64>() / n as f64;
        let std_dev = variance.sqrt();
        let upper = mid + k * std_dev;
        let lower = mid - k * std_dev;

        dates.get(i).map(|d| BollResult {
            date: d.clone(),
            upper,
            mid,
            lower,
        })
    }).collect()
}
