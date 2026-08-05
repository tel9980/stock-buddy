use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct RsiResult {
    pub date: String,
    pub rsi6: f64,
    pub rsi12: f64,
    pub rsi24: f64,
}

/// 计算 RSI 指标
pub fn calc_rsi(closes: &[f64], dates: &[String]) -> Vec<RsiResult> {
    if closes.len() < 2 {
        return Vec::new();
    }

    let calc = |period: usize, closes: &[f64]| -> Vec<f64> {
        let mut result = vec![50.0; closes.len()];
        if closes.len() < period + 1 {
            return result;
        }

        let mut avg_gain = 0.0;
        let mut avg_loss = 0.0;

        for i in 1..=period {
            let diff = closes[i] - closes[i - 1];
            if diff > 0.0 {
                avg_gain += diff;
            } else {
                avg_loss += diff.abs();
            }
        }
        avg_gain /= period as f64;
        avg_loss /= period as f64;

        for i in 1..closes.len() {
            if i < period {
                result[i] = 50.0;
                continue;
            }
            if i > period {
                let diff = closes[i] - closes[i - 1];
                let gain = if diff > 0.0 { diff } else { 0.0 };
                let loss = if diff < 0.0 { diff.abs() } else { 0.0 };
                avg_gain = (avg_gain * (period as f64 - 1.0) + gain) / period as f64;
                avg_loss = (avg_loss * (period as f64 - 1.0) + loss) / period as f64;
            }
            let rs = if avg_loss == 0.0 { 100.0 } else { avg_gain / avg_loss };
            result[i] = 100.0 - (100.0 / (1.0 + rs));
        }
        result
    };

    let rsi6 = calc(6, closes);
    let rsi12 = calc(12, closes);
    let rsi24 = calc(24, closes);

    (0..closes.len()).filter_map(|i| {
        dates.get(i).map(|d| RsiResult {
            date: d.clone(),
            rsi6: rsi6[i],
            rsi12: rsi12[i],
            rsi24: rsi24[i],
        })
    }).collect()
}
