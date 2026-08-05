use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct MacdResult {
    pub date: String,
    pub dif: f64,
    pub dea: f64,
    pub macd: f64,
}

/// 计算 MACD 指标
/// fast=12, slow=26, signal=9
pub fn calc_macd(closes: &[f64], dates: &[String], fast: usize, slow: usize, signal: usize) -> Vec<MacdResult> {
    if closes.len() < slow {
        return Vec::new();
    }

    // EMA 计算
    let ema = |period: usize, data: &[f64]| -> Vec<f64> {
        let mut result = Vec::with_capacity(data.len());
        let k = 2.0 / (period as f64 + 1.0);
        let mut prev = data[0];
        result.push(prev);
        for i in 1..data.len() {
            let val = data[i] * k + prev * (1.0 - k);
            result.push(val);
            prev = val;
        }
        result
    };

    let ema_fast = ema(fast, closes);
    let ema_slow = ema(slow, closes);

    // DIF = EMA(fast) - EMA(slow)
    let dif: Vec<f64> = (0..closes.len()).map(|i| {
        ema_fast.get(i).copied().unwrap_or(closes[i]) - ema_slow.get(i).copied().unwrap_or(closes[i])
    }).collect();

    let dea = ema(signal, &dif);

    let macd: Vec<f64> = (0..closes.len()).map(|i| (dif[i] - dea.get(i).copied().unwrap_or(0.0)) * 2.0).collect();

    (0..closes.len()).filter_map(|i| {
        if i < slow {
            None
        } else {
            dates.get(i).map(|d| MacdResult {
                date: d.clone(),
                dif: dif[i],
                dea: dea.get(i).copied().unwrap_or(0.0),
                macd: macd[i],
            })
        }
    }).collect()
}
