use serde::Serialize;
use appcore::Kline;

#[derive(Debug, Clone, Serialize)]
pub struct SupportResistance {
    pub supports: Vec<PriceLevel>,
    pub resistances: Vec<PriceLevel>,
    pub current_price: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PriceLevel {
    pub price: f64,
    pub strength: PriceStrength,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub enum PriceStrength {
    Strong,
    Medium,
    Weak,
}

/// 基于近期高低点计算支撑位/压力位
pub fn calc_support_resistance(klines: &[Kline], lookback: usize) -> SupportResistance {
    if klines.is_empty() {
        return SupportResistance {
            supports: vec![],
            resistances: vec![],
            current_price: 0.0,
        };
    }

    let lookback = lookback.min(klines.len());
    let recent = &klines[klines.len() - lookback..];
    let current_price = recent.last().map(|k| k.close).unwrap_or(0.0);

    let mut highs: Vec<f64> = recent.iter().map(|k| k.high).collect();
    let mut lows: Vec<f64> = recent.iter().map(|k| k.low).collect();
    highs.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    lows.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // 取前3个不重复的高点作为压力位
    let mut resistances = Vec::new();
    let mut prev = 0.0;
    for h in &highs {
        if (h - prev).abs() / prev.max(0.01) > 0.01 && *h > current_price {
            resistances.push(PriceLevel {
                price: *h,
                strength: if resistances.is_empty() { PriceStrength::Strong } else { PriceStrength::Medium },
                source: "近期高点".to_string(),
            });
            prev = *h;
            if resistances.len() >= 3 {
                break;
            }
        }
    }

    // 取前3个不重复的低点作为支撑位
    let mut supports = Vec::new();
    let mut prev = f64::MAX;
    for l in &lows {
        if (prev - l).abs() / prev.max(0.01) > 0.01 && *l < current_price {
            supports.push(PriceLevel {
                price: *l,
                strength: if supports.is_empty() { PriceStrength::Strong } else { PriceStrength::Medium },
                source: "近期低点".to_string(),
            });
            prev = *l;
            if supports.len() >= 3 {
                break;
            }
        }
    }

    // 整数关口
    let round = (current_price / 10.0).floor() * 10.0;
    if round > current_price {
        resistances.push(PriceLevel {
            price: round,
            strength: PriceStrength::Weak,
            source: "整数关口".to_string(),
        });
    } else if round < current_price {
        supports.push(PriceLevel {
            price: round,
            strength: PriceStrength::Weak,
            source: "整数关口".to_string(),
        });
    }

    SupportResistance {
        supports,
        resistances,
        current_price,
    }
}
