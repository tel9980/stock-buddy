use serde::Serialize;
use appcore::Kline;
use indicators::{calc_macd, calc_kdj, calc_rsi, calc_boll, calc_support_resistance, MacdResult, KdjResult, RsiResult, BollResult, SupportResistance};

/// 买卖信号类型
#[derive(Debug, Clone, Serialize)]
pub enum SignalType {
    Buy,
    Sell,
}

/// 单个买卖信号
#[derive(Debug, Clone, Serialize)]
pub struct TradeSignal {
    pub signal_type: SignalType,
    pub date: String,
    pub price: f64,
    pub source: String,
    pub strength: String,
    pub description: String,
}

/// 买卖点分析结果
#[derive(Debug, Clone, Serialize)]
pub struct SignalAnalysis {
    pub signals: Vec<TradeSignal>,
    pub current_signals: Vec<TradeSignal>,
    pub macd: Vec<MacdResult>,
    pub kdj: Vec<KdjResult>,
    pub rsi: Vec<RsiResult>,
    pub boll: Vec<BollResult>,
    pub support_resistance: SupportResistance,
    pub overall_score: i32,
    pub overall_action: String,
}

/// 分析买卖信号
pub fn analyze_signals(klines: &[Kline]) -> SignalAnalysis {
    let closes: Vec<f64> = klines.iter().map(|k| k.close).collect();
    let highs: Vec<f64> = klines.iter().map(|k| k.high).collect();
    let lows: Vec<f64> = klines.iter().map(|k| k.low).collect();
    let volumes: Vec<f64> = klines.iter().map(|k| k.volume).collect();
    let dates: Vec<String> = klines.iter().map(|k| k.date.clone()).collect();

    let macd = calc_macd(&closes, &dates, 12, 26, 9);
    let kdj = calc_kdj(&highs, &lows, &closes, &dates, 9);
    let rsi = calc_rsi(&closes, &dates);
    let boll = calc_boll(&closes, &dates, 20, 2.0);
    let sr = calc_support_resistance(klines, 60);

    let mut signals = Vec::new();
    let mut current_signals = Vec::new();

    // === MACD 金叉/死叉 ===
    // macd[0] 对应 closes[26]，即 macd[i] 对应 closes[i+26]
    for i in 1..macd.len() {
        let prev = &macd[i - 1];
        let curr = &macd[i];
        let price = closes.get(i + 26).copied().unwrap_or(0.0);

        // 金叉: DIF 从下方穿越 DEA
        if prev.dif <= prev.dea && curr.dif > curr.dea {
            signals.push(TradeSignal {
                signal_type: SignalType::Buy,
                date: curr.date.clone(),
                price,
                source: "MACD".to_string(),
                strength: if curr.macd > 0.1 { "强".into() } else { "弱".into() },
                description: format!("MACD金叉: DIF({:.4})上穿DEA({:.4})", curr.dif, curr.dea),
            });
        }

        // 死叉: DIF 从上方穿越 DEA
        if prev.dif >= prev.dea && curr.dif < curr.dea {
            signals.push(TradeSignal {
                signal_type: SignalType::Sell,
                date: curr.date.clone(),
                price,
                source: "MACD".to_string(),
                strength: if curr.macd < -0.1 { "强".into() } else { "弱".into() },
                description: format!("MACD死叉: DIF({:.4})下穿DEA({:.4})", curr.dif, curr.dea),
            });
        }

        // MACD 柱背离
        if i >= 2 {
            let prev2 = &macd[i - 2];
            if curr.macd < prev.macd && prev.macd > prev2.macd && prev.macd > 0.0 {
                let price_now = closes.get(i + 26).copied().unwrap_or(0.0);
                let price_prev = closes.get(i - 2 + 26).copied().unwrap_or(0.0);
                if price_now > price_prev {
                    signals.push(TradeSignal {
                        signal_type: SignalType::Sell,
                        date: curr.date.clone(),
                        price: price_now,
                        source: "MACD背离".to_string(),
                        strength: "中".into(),
                        description: "MACD柱顶背离: 价格新高但MACD柱缩短".to_string(),
                    });
                }
            }
            if curr.macd > prev.macd && prev.macd < prev2.macd && prev.macd < 0.0 {
                let price_now = closes.get(i + 26).copied().unwrap_or(0.0);
                let price_prev = closes.get(i - 2 + 26).copied().unwrap_or(0.0);
                if price_now < price_prev {
                    signals.push(TradeSignal {
                        signal_type: SignalType::Buy,
                        date: curr.date.clone(),
                        price: price_now,
                        source: "MACD背离".to_string(),
                        strength: "中".into(),
                        description: "MACD柱底背离: 价格新低但MACD柱缩短".to_string(),
                    });
                }
            }
        }
    }

    // === KDJ 金叉/死叉 + 超买超卖 ===
    // kdj[0] 对应 closes[8] (n=9, 第一条在 i=8)
    for i in 1..kdj.len() {
        let prev = &kdj[i - 1];
        let curr = &kdj[i];
        let price = closes.get(i + 8).copied().unwrap_or(0.0);

        // 金叉: K 从下方穿越 D
        if prev.k <= prev.d && curr.k > curr.d {
            let strength = if curr.k < 20.0 { "强" } else if curr.k < 50.0 { "中" } else { "弱" };
            signals.push(TradeSignal {
                signal_type: SignalType::Buy,
                date: curr.date.clone(),
                price,
                source: "KDJ".to_string(),
                strength: strength.into(),
                description: format!(
                    "KDJ金叉: K({:.2})上穿D({:.2}){}",
                    curr.k, curr.d,
                    if curr.k < 20.0 { "，超卖区" } else { "" }
                ),
            });
        }

        // 死叉: K 从上方穿越 D
        if prev.k >= prev.d && curr.k < curr.d {
            let strength = if curr.k > 80.0 { "强" } else if curr.k > 50.0 { "中" } else { "弱" };
            signals.push(TradeSignal {
                signal_type: SignalType::Sell,
                date: curr.date.clone(),
                price,
                source: "KDJ".to_string(),
                strength: strength.into(),
                description: format!(
                    "KDJ死叉: K({:.2})下穿D({:.2}){}",
                    curr.k, curr.d,
                    if curr.k > 80.0 { "，超买区" } else { "" }
                ),
            });
        }

        // J 值极值
        if curr.j < 0.0 && prev.j >= 0.0 {
            signals.push(TradeSignal {
                signal_type: SignalType::Buy,
                date: curr.date.clone(),
                price,
                source: "KDJ超卖".to_string(),
                strength: "中".into(),
                description: format!("KDJ超卖: J值({:.2})跌破0", curr.j),
            });
        }
        if curr.j > 100.0 && prev.j <= 100.0 {
            signals.push(TradeSignal {
                signal_type: SignalType::Sell,
                date: curr.date.clone(),
                price,
                source: "KDJ超买".to_string(),
                strength: "中".into(),
                description: format!("KDJ超买: J值({:.2})突破100", curr.j),
            });
        }
    }

    // === RSI 超买超卖 (使用 rsi6 作为短期 RSI) ===
    // rsi[0] 对应 closes[0], rsi 长度等于 closes 长度
    for i in 1..rsi.len() {
        let prev = &rsi[i - 1];
        let curr = &rsi[i];
        let price = closes.get(i).copied().unwrap_or(0.0);

        // RSI 从超卖区回升
        if prev.rsi6 < 30.0 && curr.rsi6 >= 30.0 {
            signals.push(TradeSignal {
                signal_type: SignalType::Buy,
                date: curr.date.clone(),
                price,
                source: "RSI".to_string(),
                strength: "中".into(),
                description: format!("RSI超卖回升: RSI6({:.2})回到30上方", curr.rsi6),
            });
        }

        // RSI 从超买区回落
        if prev.rsi6 > 70.0 && curr.rsi6 <= 70.0 {
            signals.push(TradeSignal {
                signal_type: SignalType::Sell,
                date: curr.date.clone(),
                price,
                source: "RSI".to_string(),
                strength: "中".into(),
                description: format!("RSI超买回落: RSI6({:.2})回到70下方", curr.rsi6),
            });
        }
    }

    // === 布林带突破 ===
    // boll[0] 对应 closes[19] (n=20, 第一条在 i=19)
    for i in 1..boll.len() {
        let prev = &boll[i - 1];
        let curr = &boll[i];
        let close_idx = i + 19;
        let prev_close = closes.get(close_idx - 1).copied().unwrap_or(0.0);
        let curr_close = closes.get(close_idx).copied().unwrap_or(0.0);

        // 跌破下轨后回升
        if prev_close < prev.lower && curr_close >= curr.lower {
            signals.push(TradeSignal {
                signal_type: SignalType::Buy,
                date: curr.date.clone(),
                price: curr_close,
                source: "布林带".to_string(),
                strength: "中".into(),
                description: format!("触及布林下轨反弹: 价格{:.2}回到下轨{:.2}上方", curr_close, curr.lower),
            });
        }

        // 突破上轨后回落
        if prev_close > prev.upper && curr_close <= curr.upper {
            signals.push(TradeSignal {
                signal_type: SignalType::Sell,
                date: curr.date.clone(),
                price: curr_close,
                source: "布林带".to_string(),
                strength: "中".into(),
                description: format!("触及布林上轨回落: 价格{:.2}回到上轨{:.2}下方", curr_close, curr.upper),
            });
        }
    }

    // === 量价信号 ===
    if klines.len() >= 10 {
        let n = klines.len();
        let recent_vol: f64 = volumes[n - 5..].iter().sum::<f64>() / 5.0;
        let prev_vol: f64 = volumes[n - 10..n - 5].iter().sum::<f64>() / 5.0;

        if prev_vol > 0.0 {
            // 放量上涨
            if recent_vol > prev_vol * 1.5 && closes[n - 1] > closes[n - 2] {
                current_signals.push(TradeSignal {
                    signal_type: SignalType::Buy,
                    date: dates[n - 1].clone(),
                    price: closes[n - 1],
                    source: "量价".to_string(),
                    strength: "中".into(),
                    description: format!("放量上涨: 近5日均量({:.0})较前5日({:.0})放大{:.0}%", recent_vol, prev_vol, (recent_vol / prev_vol - 1.0) * 100.0),
                });
            }

            // 缩量下跌
            if recent_vol < prev_vol * 0.7 && closes[n - 1] < closes[n - 2] {
                current_signals.push(TradeSignal {
                    signal_type: SignalType::Buy,
                    date: dates[n - 1].clone(),
                    price: closes[n - 1],
                    source: "量价".to_string(),
                    strength: "弱".into(),
                    description: format!("缩量下跌: 近5日均量较前5日缩小{:.0}%", (1.0 - recent_vol / prev_vol) * 100.0),
                });
            }
        }
    }

    // === 支撑/压力位信号 ===
    let current_price = closes.last().copied().unwrap_or(0.0);
    if current_price > 0.0 {
        for s in &sr.supports {
            if (current_price - s.price).abs() / current_price < 0.02 {
                current_signals.push(TradeSignal {
                    signal_type: SignalType::Buy,
                    date: dates.last().cloned().unwrap_or_default(),
                    price: s.price,
                    source: "支撑位".to_string(),
                    strength: format!("{:?}", s.strength),
                    description: format!("接近支撑位{:.2}({})", s.price, s.source),
                });
            }
        }
        for r in &sr.resistances {
            if (current_price - r.price).abs() / current_price < 0.02 {
                current_signals.push(TradeSignal {
                    signal_type: SignalType::Sell,
                    date: dates.last().cloned().unwrap_or_default(),
                    price: r.price,
                    source: "压力位".to_string(),
                    strength: format!("{:?}", r.strength),
                    description: format!("接近压力位{:.2}({})", r.price, r.source),
                });
            }
        }
    }

    // === 最新信号检测（最近5根K线）===
    let recent_dates: std::collections::HashSet<&str> = dates.iter().rev().take(5).map(|s| s.as_str()).collect();
    for s in &signals {
        if recent_dates.contains(s.date.as_str()) {
            current_signals.push(s.clone());
        }
    }

    // === 综合评分 ===
    let mut score: i32 = 0;
    // MACD 趋势
    if let Some(m) = macd.last() {
        if m.dif > m.dea { score += 2; } else { score -= 2; }
        if m.macd > 0.0 { score += 1; } else { score -= 1; }
    }
    // KDJ 趋势
    if let Some(k) = kdj.last() {
        if k.k > k.d { score += 1; } else { score -= 1; }
        if k.k < 20.0 { score += 2; }
        if k.k > 80.0 { score -= 2; }
    }
    // RSI 趋势 (使用 rsi12 作为中期 RSI)
    if let Some(r) = rsi.last() {
        if r.rsi12 < 30.0 { score += 2; }
        if r.rsi12 > 70.0 { score -= 2; }
        if r.rsi12 > 50.0 { score += 1; } else { score -= 1; }
    }
    // 布林带位置
    if let (Some(b), Some(&cp)) = (boll.last(), closes.last()) {
        if cp < b.lower { score += 1; }
        if cp > b.upper { score -= 1; }
    }
    // 当前信号
    for s in &current_signals {
        match s.signal_type {
            SignalType::Buy => score += match s.strength.as_str() {
                "强" => 3, "中" => 2, _ => 1,
            },
            SignalType::Sell => score -= match s.strength.as_str() {
                "强" => 3, "中" => 2, _ => 1,
            },
        }
    }

    let overall_action = if score >= 4 {
        "强烈买入".to_string()
    } else if score >= 1 {
        "偏多/可买入".to_string()
    } else if score <= -4 {
        "强烈卖出".to_string()
    } else if score <= -1 {
        "偏空/宜减仓".to_string()
    } else {
        "中性观望".to_string()
    };

    SignalAnalysis {
        signals,
        current_signals,
        macd,
        kdj,
        rsi,
        boll,
        support_resistance: sr,
        overall_score: score,
        overall_action,
    }
}
