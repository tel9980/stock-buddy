//! 盘口解读（Tape Reading）模块
//!
//! 用聚合数据近似书里的「盘口解读」：真实盘口看的是逐笔订单流与成交节奏，
//! 本项目没有 tick 级数据，因此用已有的两类数据交叉验证量价信号，判断
//! 「放量」究竟是机构真买，还是对倒诱多 / 散户追涨 / 主力出货：
//!
//! 1. 资金流向（按单笔金额拆分的 超大单/大单/中单/小单 净流入）—— 代理「主动性大单买卖」。
//! 2. 龙虎榜（上榜席位性质、机构净买、游资标签）—— 确认是否真有机构参与。
//!
//! 交叉逻辑：
//! - 量增价升 + 主力(超大单+大单)净流入为正且占比高 + (机构席位净买) → 机构真买（看多）。
//! - 量增价升 但 主力净流出、小单净流入为正 → 散户追涨、主力出货（谨慎/看空）。
//! - 放量 但 超大单与大单买卖都大、净额却近零 → 对倒嫌疑（诱多）。
//! - 量增价跌 + 主力净流出 → 主力出货（看空）。
//! - 龙虎榜机构净买为正 → 确认机构吸筹；游资标签为主 → 偏投机、持续性差。

use serde::Serialize;

use appcore::CapitalFlow;
use appcore::DragonTigerEntry;

use crate::volume_price::VolumePriceAnalysis;

/// 单日资金流观测点（用于前端小图）
#[derive(Debug, Clone, Serialize)]
pub struct FlowPoint {
    pub date: String,
    /// 主力净流入（万元）
    pub main_net_inflow: f64,
    /// 主力净流入占比 (%)
    pub main_net_ratio: f64,
    /// 涨跌幅 (%)
    pub pct_change: f64,
}

/// 龙虎榜席位摘要
#[derive(Debug, Clone, Serialize)]
pub struct DragonTigerSummary {
    pub date: String,
    /// 上榜原因
    pub reason: String,
    /// 龙虎榜净买入（万元）
    pub net_amount: f64,
    /// 机构席位净买入（万元）
    pub institution_net_buy: f64,
    /// 游资标签
    pub hot_money_tags: Vec<String>,
}

/// 盘口解读信号
#[derive(Debug, Clone, Serialize)]
pub struct TapeReadingSignal {
    pub category: String,
    pub name: String,
    pub strength: i32,
    pub detail: String,
}

/// 盘口解读完整结果
#[derive(Debug, Clone, Serialize)]
pub struct TapeReadingAnalysis {
    pub stock_code: String,
    /// 盘口 verdict：机构主导(看多) / 机构吸筹(看多) / 游资主导(关注) /
    /// 散户推动(谨慎) / 主力出货(看空) / 对倒诱多(看空) / 数据不足
    pub verdict: String,
    /// 置信度：高 / 中 / 低
    pub confidence: String,
    /// 主力净流入占成交额比（最新，%）
    pub main_participation: f64,
    /// 超大单+大单净流入（万元）
    pub big_order_net: f64,
    /// 是否存在对倒嫌疑
    pub wash_suspect: bool,
    /// 是否散户推动（主力未买、小单主导）
    pub retail_driven: bool,
    /// 近期资金流观测点
    pub recent_flow: Vec<FlowPoint>,
    /// 龙虎榜席位摘要（若上榜）
    pub dragon_tiger: Option<DragonTigerSummary>,
    /// 关键信号
    pub signals: Vec<TapeReadingSignal>,
    /// 中文结论
    pub conclusion: String,
}

/// 盘口解读主函数
pub fn analyze_tape_reading(
    code: &str,
    vp: &VolumePriceAnalysis,
    capital_flow: &[CapitalFlow],
    dragon_tiger: Option<&DragonTigerEntry>,
) -> TapeReadingAnalysis {
    let latest = capital_flow.last();

    if latest.is_none() {
        return TapeReadingAnalysis {
            stock_code: code.to_string(),
            verdict: "数据不足".to_string(),
            confidence: "低".to_string(),
            main_participation: 0.0,
            big_order_net: 0.0,
            wash_suspect: false,
            retail_driven: false,
            recent_flow: recent_flow_points(capital_flow),
            dragon_tiger: dragon_tiger.map(summarize_dt),
            signals: vec![TapeReadingSignal {
                category: "中性".to_string(),
                name: "资金流向缺失".to_string(),
                strength: 1,
                detail: "未获取到个股资金流向数据，无法判断盘口主动性买卖结构。".to_string(),
            }],
            conclusion: "缺少资金流向数据，盘口解读暂时无法给出。请稍后重试或确认该标的支持资金流查询。".to_string(),
        };
    }

    let cf = latest.unwrap();
    let big_order_net = cf.huge_net_inflow + cf.large_net_inflow;
    let main_participation = cf.main_net_ratio;

    // ── 对倒嫌疑：放量 + 超大单/大单买卖都大但净额近零 ──
    let gross_big = cf.huge_net_inflow.abs() + cf.large_net_inflow.abs();
    let net_big = big_order_net;
    let wash_suspect = vp.volume_status == "放量"
        && gross_big > 0.0
        && net_big.abs() < gross_big * 0.25;

    // ── 散户推动：价涨量增但主力没买、小单在买 ──
    let vp_bullish = matches!(vp.verdict.as_str(), "看多");
    let retail_driven = vp_bullish
        && cf.main_net_inflow <= 0.0
        && cf.small_net_inflow > 0.0
        && cf.huge_net_inflow + cf.large_net_inflow <= 0.0;

    let mut signals: Vec<TapeReadingSignal> = Vec::new();

    // 主力净流入方向
    if cf.main_net_inflow > 0.0 {
        signals.push(TapeReadingSignal {
            category: "偏多".to_string(),
            name: "主力净流入".to_string(),
            strength: if main_participation >= 5.0 { 4 } else { 3 },
            detail: format!(
                "最新交易日主力净流入 {:.2} 万元，占成交额 {:.2}%，{}.",
                cf.main_net_inflow,
                main_participation,
                if main_participation >= 5.0 {
                    "占比显著，机构主动性买盘较强"
                } else {
                    "有一定参与度"
                }
            ),
        });
    } else if cf.main_net_inflow < 0.0 {
        signals.push(TapeReadingSignal {
            category: "偏空".to_string(),
            name: "主力净流出".to_string(),
            strength: if main_participation <= -5.0 { 4 } else { 3 },
            detail: format!(
                "最新交易日主力净流出 {:.2} 万元，占成交额 {:.2}%，资金呈撤离迹象。",
                cf.main_net_inflow.abs(),
                main_participation.abs()
            ),
        });
    }

    // 超大单/大单结构
    if big_order_net > 0.0 {
        signals.push(TapeReadingSignal {
            category: "偏多".to_string(),
            name: "大资金净买入".to_string(),
            strength: 3,
            detail: format!(
                "超大单+大单合计净买入 {:.2} 万元（超大单 {:.2}，大单 {:.2}），大单主动性买盘为主。",
                big_order_net, cf.huge_net_inflow, cf.large_net_inflow
            ),
        });
    } else if big_order_net < 0.0 {
        signals.push(TapeReadingSignal {
            category: "偏空".to_string(),
            name: "大资金净卖出".to_string(),
            strength: 3,
            detail: format!(
                "超大单+大单合计净卖出 {:.2} 万元（超大单 {:.2}，大单 {:.2}），大资金逢高派发。",
                big_order_net.abs(), cf.huge_net_inflow, cf.large_net_inflow
            ),
        });
    }

    if retail_driven {
        signals.push(TapeReadingSignal {
            category: "关注".to_string(),
            name: "散户推动".to_string(),
            strength: 4,
            detail: format!(
                "价格{}但主力净流出、小单净流入 {:.2} 万元，上涨主要由散户追涨驱动，持续性差、易回落。",
                cf.pct_change,
                cf.small_net_inflow
            ),
        });
    }

    if wash_suspect {
        signals.push(TapeReadingSignal {
            category: "关注".to_string(),
            name: "对倒嫌疑".to_string(),
            strength: 4,
            detail: format!(
                "成交放量（量比 {:.2}）但超大单与大单买卖额都大、净额却仅 {:.2} 万元，双向对敲制造活跃假象，警惕诱多。",
                vp.current_volume_ratio, net_big
            ),
        });
    }

    // 龙虎榜验证
    let mut dt_confirm = None;
    if let Some(dt) = dragon_tiger {
        if dt.institution_net_buy > 0.0 {
            signals.push(TapeReadingSignal {
                category: "偏多".to_string(),
                name: "机构席位净买".to_string(),
                strength: 4,
                detail: format!(
                    "龙虎榜({})机构席位净买入 {:.2} 万元，确认有机构真实参与吸筹。",
                    dt.date, dt.institution_net_buy
                ),
            });
            dt_confirm = Some(true);
        } else if dt.institution_net_buy < 0.0 {
            signals.push(TapeReadingSignal {
                category: "偏空".to_string(),
                name: "机构席位净卖".to_string(),
                strength: 4,
                detail: format!(
                    "龙虎榜({})机构席位净卖出 {:.2} 万元，虽上榜净买但机构已派发，需警惕。",
                    dt.date, dt.institution_net_buy.abs()
                ),
            });
            dt_confirm = Some(false);
        }
        if !dt.hot_money_tags.is_empty() {
            signals.push(TapeReadingSignal {
                category: "关注".to_string(),
                name: "游资主导".to_string(),
                strength: 3,
                detail: format!(
                    "龙虎榜出现游资标签：{}，多为短线博弈，行情持续性通常弱于机构主导。",
                    dt.hot_money_tags.join("、")
                ),
            });
        }
    }

    // ── 综合 verdict ──
    let (verdict, confidence, conclusion) = decide_verdict(
        vp,
        cf,
        dragon_tiger,
        wash_suspect,
        retail_driven,
        dt_confirm,
        &signals,
    );

    TapeReadingAnalysis {
        stock_code: code.to_string(),
        verdict,
        confidence,
        main_participation,
        big_order_net,
        wash_suspect,
        retail_driven,
        recent_flow: recent_flow_points(capital_flow),
        dragon_tiger: dragon_tiger.map(summarize_dt),
        signals,
        conclusion,
    }
}

fn recent_flow_points(capital_flow: &[CapitalFlow]) -> Vec<FlowPoint> {
    capital_flow
        .iter()
        .rev()
        .take(10)
        .rev()
        .map(|c| FlowPoint {
            date: c.date.clone(),
            main_net_inflow: c.main_net_inflow,
            main_net_ratio: c.main_net_ratio,
            pct_change: c.pct_change,
        })
        .collect()
}

fn summarize_dt(dt: &DragonTigerEntry) -> DragonTigerSummary {
    DragonTigerSummary {
        date: dt.date.clone(),
        reason: dt.reason.clone(),
        net_amount: dt.net_amount,
        institution_net_buy: dt.institution_net_buy,
        hot_money_tags: dt.hot_money_tags.clone(),
    }
}

#[allow(clippy::too_many_arguments)]
fn decide_verdict(
    vp: &VolumePriceAnalysis,
    cf: &CapitalFlow,
    dt: Option<&DragonTigerEntry>,
    wash_suspect: bool,
    retail_driven: bool,
    dt_confirm: Option<bool>,
    signals: &[TapeReadingSignal],
) -> (String, String, String) {
    let vp_bullish = matches!(vp.verdict.as_str(), "看多");
    let vp_bearish = matches!(vp.verdict.as_str(), "看空");
    let big_positive = cf.huge_net_inflow + cf.large_net_inflow > 0.0;
    let main_positive = cf.main_net_inflow > 0.0;

    // 1) 对倒诱多：最危险，优先判定
    if wash_suspect {
        return (
            "对倒诱多".to_string(),
            "中".to_string(),
            format!(
                "量价显示{}，但资金流呈现明显对倒特征——成交放量而大单净额近零，疑似自买自卖制造活跃假象。\
                 书里强调这种「放量无量价支撑」的盘口是典型诱多，建议观望或减仓，不追高。",
                vp.summary
            ),
        );
    }

    // 2) 散户推动
    if retail_driven {
        return (
            "散户推动".to_string(),
            "中".to_string(),
            format!(
                "量价{}但主力资金未进场（主力净流出、小单净流入为正），上涨由散户追涨推动，动能虚、持续性差。\
                 盘口上属于「价涨而大单不出力」，遇阻易回落，谨慎追高。",
                vp.summary
            ),
        );
    }

    // 3) 机构主导 / 吸筹（看多）
    if vp_bullish && main_positive && big_positive {
        let mut c = format!(
            "量价{}得到资金面确认：主力净流入 {:.2} 万元（占比 {:.2}%），超大单+大单净买入 {:.2} 万元，\
             是大单主动性买盘推动的健康上涨，符合书里「放量上涨且由机构真买驱动」的强势盘口。",
            vp.summary, cf.main_net_inflow, cf.main_net_ratio, cf.huge_net_inflow + cf.large_net_inflow
        );
        if dt_confirm == Some(true) {
            c.push_str("龙虎榜机构席位净买进一步确认机构吸筹，信号较强。");
        } else if dt_confirm == Some(false) {
            c.push_str("但龙虎榜显示机构席位净卖出，机构内部有分歧，注意上方抛压。");
        } else {
            c.push_str("（该股当日未上龙虎榜，结论基于资金流与量价。）");
        }
        let conf = if dt_confirm == Some(true) { "高" } else { "中" };
        return ("机构主导".to_string(), conf.to_string(), c);
    }

    // 4) 主力出货（看空）
    if (vp_bearish || matches!(vp.volume_status.as_str(), "放量"))
        && cf.main_net_inflow < 0.0
        && cf.huge_net_inflow + cf.large_net_inflow < 0.0
    {
        let mut c = format!(
            "盘口呈派发特征：量价{}且主力净流出 {:.2} 万元、超大单+大单合计净卖出 {:.2} 万元，\
             大资金在上涨/震荡中逢高出货，是书里典型的「主力出货」盘口。",
            vp.summary, cf.main_net_inflow.abs(), (cf.huge_net_inflow + cf.large_net_inflow).abs()
        );
        if dt_confirm == Some(false) {
            c.push_str("龙虎榜机构席位亦净卖出，出货信号叠加，偏空。");
        }
        return ("主力出货".to_string(), "中".to_string(), c);
    }

    // 5) 游资主导
    if let Some(d) = dt {
        if !d.hot_money_tags.is_empty() && d.institution_net_buy <= 0.0 {
            return (
                "游资主导".to_string(),
                "中".to_string(),
                format!(
                    "龙虎榜显示游资主导（标签：{}），机构未明显参与。游资行情短线博弈性强、持续性弱，\
                     盘口波动大，宜快进快出、控制仓位。",
                    d.hot_money_tags.join("、")
                ),
            );
        }
    }

    // 6) 中性 / 信号冲突
    let conf = if signals.is_empty() { "低" } else { "中" };
    let c = format!(
        "量价与资金流未形成明确合力（量价：{}；主力净流入 {:.2} 万元），盘口信号偏中性。\
         建议结合支撑/压力位与大盘环境综合判断，不单独据此操作。",
        vp.summary, cf.main_net_inflow
    );
    ("中性".to_string(), conf.to_string(), c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::volume_price::VpBar;

    fn vp(verdict: &str, volume_status: &str, vol_ratio: f64, summary: &str) -> VolumePriceAnalysis {
        VolumePriceAnalysis {
            summary: summary.to_string(),
            verdict: verdict.to_string(),
            volume_status: volume_status.to_string(),
            current_volume_ratio: vol_ratio,
            recent_bars: vec![VpBar {
                date: "2026-08-12".to_string(),
                close: 10.0,
                volume: 1000.0,
                amount: 1.0,
                pct_change: 3.0,
                volume_ratio: vol_ratio,
                relationship: crate::volume_price::VpRelationship::VolumeUpPriceUp,
                note: "".to_string(),
            }],
            signals: vec![],
            conclusion: "".to_string(),
        }
    }

    fn cf(main: f64, huge: f64, large: f64, small: f64, ratio: f64) -> CapitalFlow {
        CapitalFlow {
            stock_code: "000001".to_string(),
            date: "2026-08-12".to_string(),
            main_net_inflow: main,
            main_net_ratio: ratio,
            huge_net_inflow: huge,
            huge_net_ratio: 0.0,
            large_net_inflow: large,
            large_net_ratio: 0.0,
            medium_net_inflow: 0.0,
            medium_net_ratio: 0.0,
            small_net_inflow: small,
            small_net_ratio: 0.0,
            close_price: 10.0,
            pct_change: 3.0,
        }
    }

    #[test]
    fn test_institution_driven() {
        let a = analyze_tape_reading(
            "000001",
            &vp("看多", "放量", 2.0, "量增价升"),
            &[cf(5000.0, 3000.0, 2000.0, -1000.0, 6.0)],
            None,
        );
        assert_eq!(a.verdict, "机构主导");
        assert!(a.big_order_net > 0.0);
    }

    #[test]
    fn test_retail_driven() {
        let a = analyze_tape_reading(
            "000001",
            &vp("看多", "放量", 2.0, "量增价升"),
            &[cf(-500.0, -300.0, -200.0, 800.0, -0.6)],
            None,
        );
        assert_eq!(a.verdict, "散户推动");
        assert!(a.retail_driven);
    }

    #[test]
    fn test_wash_suspect() {
        let a = analyze_tape_reading(
            "000001",
            &vp("看多", "放量", 2.5, "量增价升"),
            // 超大单与大单都大(±5000)但净额近零
            &[cf(50.0, 5000.0, -4950.0, 0.0, 0.1)],
            None,
        );
        assert_eq!(a.verdict, "对倒诱多");
        assert!(a.wash_suspect);
    }

    #[test]
    fn test_main_force_distribution() {
        let a = analyze_tape_reading(
            "000001",
            &vp("看空", "放量", 2.0, "量增价跌"),
            &[cf(-4000.0, -2500.0, -1500.0, 1000.0, -5.0)],
            None,
        );
        assert_eq!(a.verdict, "主力出货");
    }

    #[test]
    fn test_no_capital_flow() {
        let a = analyze_tape_reading("000001", &vp("看多", "放量", 2.0, "量增价升"), &[], None);
        assert_eq!(a.verdict, "数据不足");
    }

    #[test]
    fn test_institution_confirmed_by_dragon_tiger() {
        let dt = DragonTigerEntry {
            code: "000001".to_string(),
            name: "测试".to_string(),
            date: "2026-08-12".to_string(),
            close_price: 10.0,
            pct_change: 3.0,
            turnover: 10.0,
            amount: 100000.0,
            amplitude: 5.0,
            net_amount: 3000.0,
            total_buy: 8000.0,
            total_sell: 5000.0,
            lhb_amount_ratio: 12.0,
            reason: "日涨幅偏离值达7%".to_string(),
            buy_seats: vec![],
            sell_seats: vec![],
            institution_net_buy: 2000.0,
            hot_money_tags: vec![],
        };
        let a = analyze_tape_reading(
            "000001",
            &vp("看多", "放量", 2.0, "量增价升"),
            &[cf(5000.0, 3000.0, 2000.0, -1000.0, 6.0)],
            Some(&dt),
        );
        assert_eq!(a.verdict, "机构主导");
        assert_eq!(a.confidence, "高");
        assert!(a.dragon_tiger.is_some());
    }
}
