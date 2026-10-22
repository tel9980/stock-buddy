//! 市场热点板块分析
//!
//! 聚合板块排行、板块资金流、龙虎榜与领涨股基本面，对每个热点板块给出
//! 「技术面 + 基本面 + 催化剂」三维上涨原因，并按综合热度评分排序。
//!
//! - 技术面：板块涨幅、领涨股带动、板块内上涨广度、主力资金净流入确认。
//! - 基本面：领涨股最新报告期业绩（营收/净利增速、ROE、毛利率），判断上涨是否由业绩支撑。
//! - 催化剂：领涨股是否上龙虎榜、机构席位净买或游资标签，判断短线驱动性质。

use std::collections::HashMap;

use appcore::models::{
    DragonTigerEntry, FinancialSummary, ReasonFactor, SectorHotspotAnalysis, SectorHotspotItem,
    SectorLeaderFundamental, SectorRank, SectorCapitalFlow, SectorUpReason,
};

/// 评分中间结果（路由据其决定抓取哪些领涨股基本面，再交给 `analyze_sector_hotspot`）
#[derive(Debug, Clone)]
pub struct HotSectorSeed {
    pub code: String,
    pub name: String,
    pub sector_type: String,
    pub change_pct: f64,
    /// 领涨股名称，仅用于展示
    pub leader_stock: Option<String>,
    /// 领涨股代码，是后续一切「按股票查询」的键（财报、龙虎榜）
    pub leader_code: Option<String>,
    pub leader_pct: Option<f64>,
    pub up_count: u32,
    pub down_count: u32,
    pub main_net_inflow: f64,
    pub main_net_ratio: f64,
    pub strength_score: f64,
}

/// 合并行业 + 概念板块，按「涨幅 + 主力净流入 + 上涨广度」综合热度评分，
/// 取前 `top_n` 个作为热点板块候选。
pub fn score_hot_sectors(
    industry: &[SectorRank],
    concept: &[SectorRank],
    flows: &[SectorCapitalFlow],
    top_n: usize,
) -> Vec<HotSectorSeed> {
    let flow_map: HashMap<&str, &SectorCapitalFlow> =
        flows.iter().map(|f| (f.code.as_str(), f)).collect();

    let mut candidates: Vec<(SectorRank, &str)> = Vec::new();
    for r in industry {
        candidates.push((r.clone(), "行业"));
    }
    for r in concept {
        candidates.push((r.clone(), "概念"));
    }

    // 归一化基准：用候选集合内的最大值，避免对量纲（元/万）做硬编码假设
    let max_change = candidates
        .iter()
        .map(|(r, _)| r.change_pct)
        .filter(|&c| c > 0.0)
        .fold(0.1f64, f64::max);
    let max_inflow = candidates
        .iter()
        .map(|(r, _)| {
            flow_map
                .get(r.code.as_str())
                .map(|f| f.main_net_inflow)
                .unwrap_or(0.0)
        })
        .filter(|&v| v > 0.0)
        .fold(0.0f64, f64::max);

    let mut scored: Vec<HotSectorSeed> = candidates
        .into_iter()
        .map(|(r, stype)| {
            let flow = flow_map.get(r.code.as_str());
            let inflow = flow.map(|f| f.main_net_inflow).unwrap_or(0.0);
            let inflow_ratio = flow.map(|f| f.main_net_ratio).unwrap_or(0.0);

            let up = r.up_count.unwrap_or(0);
            let down = r.down_count.unwrap_or(0);

            let change_c = if r.change_pct > 0.0 {
                (r.change_pct / max_change).min(1.0) * 100.0
            } else {
                0.0
            };
            let inflow_c = if max_inflow > 0.0 {
                (inflow / max_inflow).max(0.0).min(1.0) * 100.0
            } else {
                0.0
            };
            let total = up + down;
            let breadth_c = if total > 0 {
                up as f64 / total as f64 * 40.0
            } else {
                0.0
            };

            let strength = 0.45 * change_c + 0.35 * inflow_c + 0.20 * breadth_c;

            HotSectorSeed {
                code: r.code,
                name: r.name,
                sector_type: stype.to_string(),
                change_pct: r.change_pct,
                leader_stock: r.leader_stock,
                leader_code: r.leader_code,
                leader_pct: r.leader_pct,
                up_count: up,
                down_count: down,
                main_net_inflow: inflow,
                main_net_ratio: inflow_ratio,
                strength_score: strength,
            }
        })
        .collect();

    scored.sort_by(|a, b| {
        b.strength_score
            .partial_cmp(&a.strength_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(top_n);
    scored
}

/// 对评分后的热点板块候选生成上涨原因分析。
///
/// - `seeds`：`score_hot_sectors` 产出的前 N 个板块。
/// - `financials`：领涨股代码 -> 基本面（由路由并发抓取后传入）。
/// - `dragon_tiger`：当日龙虎榜列表（用于匹配领涨股催化信号）。
/// - `total_industry` / `total_concept`：全市场行业/概念板块总数（用于结论中的整体口径）。
pub fn analyze_sector_hotspot(
    seeds: &[HotSectorSeed],
    financials: &HashMap<String, FinancialSummary>,
    dragon_tiger: &[DragonTigerEntry],
    total_industry: usize,
    total_concept: usize,
) -> SectorHotspotAnalysis {
    let generated_at = chrono::Utc::now().timestamp();

    let mut items: Vec<SectorHotspotItem> = Vec::with_capacity(seeds.len());

    for (i, seed) in seeds.iter().enumerate() {
        let inflow_yi = seed.main_net_inflow / 1e8;
        let breadth_total = seed.up_count + seed.down_count;
        let breadth_ratio = if breadth_total > 0 {
            seed.up_count as f64 / breadth_total as f64
        } else {
            0.0
        };

        // 龙虎榜命中（领涨股）
        //
        // 必须按 **代码** 匹配：龙虎榜条目存的是 6 位代码，而 `leader_stock`
        // 是名称，早年拿名称来比对导致这里恒为 false，催化信号从未生效。
        let dt_entry = seed
            .leader_code
            .as_ref()
            .and_then(|c| dragon_tiger.iter().find(|d| &d.code == c));
        let dragon_tiger_hit = dt_entry.is_some();

        // 领涨股基本面（仅当真实返回了财务指标时才算作有基本面数据）
        //
        // `financials` 由路由按领涨股**代码**抓取并建键，这里必须用同一个键去取。
        let leader_fund = seed.leader_code.as_ref().and_then(|code| {
            financials
                .get(code)
                .filter(|fin| !fin.indicators.is_empty())
                .map(|fin| build_leader_fundamental(code, fin, seed.leader_pct))
        });

        // ── 技术面 ──
        let mut technical = format!(
            "板块涨幅 {:.2}%，{}{}全市场前列；领涨股 {} 上涨 {:.2}%，对板块形成带动。",
            seed.change_pct,
            if seed.change_pct >= 3.0 { "居" } else { "处于" },
            if seed.change_pct >= 3.0 { "" } else { "相对" },
            seed.leader_stock.as_deref().unwrap_or("无"),
            seed.leader_pct.unwrap_or(0.0)
        );
        if breadth_total > 0 {
            if breadth_ratio >= 0.7 {
                technical.push_str(&format!(
                    "板块内 {} 只上涨、{} 只下跌，普涨格局、广度健康，并非单点拉动。",
                    seed.up_count, seed.down_count
                ));
            } else if breadth_ratio >= 0.4 {
                technical.push_str(&format!(
                    "板块内 {} 涨 {} 跌，多数个股跟涨，热度中等。",
                    seed.up_count, seed.down_count
                ));
            } else {
                technical.push_str(&format!(
                    "板块内 {} 涨 {} 跌，上涨集中于少数龙头，广度偏弱、分化明显。",
                    seed.up_count, seed.down_count
                ));
            }
        }
        if inflow_yi.abs() >= 0.01 {
            technical.push_str(&format!(
                "主力资金{} {:.2} 亿元（占成交额 {:.2}%），{}。",
                if inflow_yi >= 0.0 { "净流入" } else { "净流出" },
                inflow_yi.abs(),
                seed.main_net_ratio,
                if inflow_yi > 0.0 {
                    "资金面确认板块走强"
                } else {
                    "资金面偏谨慎"
                }
            ));
        }

        // ── 基本面 + 上涨逻辑定性 ──
        let mut fundamental = String::new();
        let pct = |o: Option<f64>| o.map(|v| format!("{:.1}%", v)).unwrap_or_else(|| "—".to_string());
        let mut logic = if let Some(lf) = &leader_fund {
            let growth = lf.net_profit_yoy;
            fundamental.push_str(&format!(
                "领涨股 {}（最新报告期 {}）营收同比 {}、归母净利润同比 {}、ROE {}、毛利率 {}。",
                lf.name,
                lf.report_date,
                pct(lf.revenue_yoy),
                pct(growth),
                pct(lf.roe),
                pct(lf.gross_margin)
            ));
            match growth {
                Some(g) if g >= 30.0 && inflow_yi > 0.0 => {
                    fundamental.push_str(
                        "业绩高增叠加主力资金流入，板块上涨具备明确基本面支撑，属「业绩驱动」逻辑。",
                    );
                    "业绩驱动"
                }
                Some(g) if g >= 0.0 => {
                    fundamental.push_str(
                        "业绩稳健（正增长）且资金流入，基本面未明显恶化，可视为「业绩+资金」共同推动。",
                    );
                    if inflow_yi > 0.0 {
                        "业绩+资金驱动"
                    } else {
                        "业绩支撑"
                    }
                }
                Some(_) => {
                    fundamental.push_str(
                        "领涨股业绩同比下滑，基本面偏弱；当前上涨更可能由资金或题材驱动，持续性需警惕。",
                    );
                    if dragon_tiger_hit {
                        "题材炒作"
                    } else {
                        "资金驱动"
                    }
                }
                None => {
                    fundamental.push_str(
                        "（业绩增速数据缺失，基本面验证不充分，结论以技术面与资金面为主。）",
                    );
                    if dragon_tiger_hit {
                        "题材/游资炒作"
                    } else if inflow_yi > 0.0 {
                        "资金驱动"
                    } else {
                        "普涨跟随"
                    }
                }
            }
        } else {
            fundamental.push_str(
                "未能获取领涨股基本面数据（财务接口当前未返回数据或领涨股未知）。基本面结论暂不可靠，以下分析以技术面与资金面为主。",
            );
            if dragon_tiger_hit {
                "题材/游资炒作"
            } else if inflow_yi > 0.0 {
                "资金驱动"
            } else {
                "普涨跟随"
            }
        };

        // ── 催化剂 ──
        let mut catalyst = String::new();
        if let Some(dt) = dt_entry {
            if dt.institution_net_buy > 0.0 {
                catalyst.push_str(&format!(
                    "领涨股 {} 登上龙虎榜（{}），机构席位净买入 {:.2} 万元，确认机构参与，构成基本面/资金面共振催化。",
                    dt.code, dt.reason, dt.institution_net_buy
                ));
            } else if dt.institution_net_buy < 0.0 {
                catalyst.push_str(&format!(
                    "领涨股 {} 登上龙虎榜（{}），但机构席位净卖出 {:.2} 万元，机构偏谨慎。",
                    dt.code,
                    dt.reason,
                    dt.institution_net_buy.abs()
                ));
            }
            if !dt.hot_money_tags.is_empty() {
                catalyst.push_str(&format!(
                    "龙虎榜出现游资标签（{}），属短线博弈催化，持续性通常弱于机构主导。",
                    dt.hot_money_tags.join("、")
                ));
                if logic != "业绩驱动" && logic != "业绩+资金驱动" {
                    logic = "题材/游资炒作";
                }
            }
        } else {
            catalyst.push_str(
                "当日龙虎榜未见领涨股上榜，无明显短线席位催化信号。若属政策或行业事件驱动，需结合新闻面判断（本项目暂未接入新闻源）。",
            );
        }

        // ── 驱动因子（贡献度） ──
        let mut factors: Vec<ReasonFactor> = Vec::new();
        factors.push(ReasonFactor {
            category: "技术面".to_string(),
            name: "涨幅强度".to_string(),
            weight: (seed.change_pct / 5.0).clamp(0.0, 1.0),
            detail: format!("板块涨幅 {:.2}%", seed.change_pct),
        });
        factors.push(ReasonFactor {
            category: "资金面".to_string(),
            name: "主力净流入".to_string(),
            weight: if inflow_yi > 0.0 {
                (inflow_yi / 2.0).clamp(0.0, 1.0)
            } else {
                0.0
            },
            detail: format!("主力净流入 {:.2} 亿", inflow_yi),
        });
        if breadth_total > 0 {
            factors.push(ReasonFactor {
                category: "技术面".to_string(),
                name: "上涨广度".to_string(),
                weight: breadth_ratio,
                detail: format!("{} 涨 {} 跌", seed.up_count, seed.down_count),
            });
        }
        if let Some(lf) = &leader_fund {
            if let Some(g) = lf.net_profit_yoy {
                factors.push(ReasonFactor {
                    category: "基本面".to_string(),
                    name: "业绩增速".to_string(),
                    weight: (g / 50.0).clamp(0.0, 1.0),
                    detail: format!("净利同比 {:.1}%", g),
                });
            }
        }
        if dragon_tiger_hit {
            factors.push(ReasonFactor {
                category: "催化剂".to_string(),
                name: "龙虎榜".to_string(),
                weight: 0.6,
                detail: "领涨股上榜".to_string(),
            });
        }

        let summary = format!(
            "【{}】{}：{}（涨幅 {:.2}%，主力净流入 {:.2} 亿）",
            seed.sector_type, seed.name, logic, seed.change_pct, inflow_yi
        );

        items.push(SectorHotspotItem {
            rank: i + 1,
            code: seed.code.clone(),
            name: seed.name.clone(),
            sector_type: seed.sector_type.clone(),
            change_pct: seed.change_pct,
            leader_stock: seed.leader_stock.clone(),
            leader_code: seed.leader_code.clone(),
            leader_pct: seed.leader_pct,
            up_count: seed.up_count,
            down_count: seed.down_count,
            main_net_inflow: seed.main_net_inflow,
            main_net_ratio: seed.main_net_ratio,
            strength_score: seed.strength_score,
            leader_fundamental: leader_fund,
            dragon_tiger_hit,
            reason: SectorUpReason {
                logic: logic.to_string(),
                technical,
                fundamental,
                catalyst,
                factors,
            },
            summary,
        });
    }

    let conclusion = build_conclusion(&items, total_industry, total_concept);

    SectorHotspotAnalysis {
        generated_at,
        industry_count: total_industry,
        concept_count: total_concept,
        hot_sectors: items,
        conclusion,
    }
}

fn build_leader_fundamental(
    code: &str,
    fin: &FinancialSummary,
    pct: Option<f64>,
) -> SectorLeaderFundamental {
    let latest = fin.indicators.first();
    SectorLeaderFundamental {
        code: code.to_string(),
        name: fin.stock_name.clone(),
        pct_change: pct,
        revenue_yoy: latest.and_then(|i| i.revenue_yoy),
        net_profit_yoy: latest.and_then(|i| i.net_profit_yoy),
        roe: latest.and_then(|i| i.roe),
        gross_margin: latest.and_then(|i| i.gross_margin),
        net_margin: latest.and_then(|i| i.net_margin),
        report_date: latest.map(|i| i.report_date.clone()).unwrap_or_default(),
    }
}

fn build_conclusion(
    items: &[SectorHotspotItem],
    industry_count: usize,
    concept_count: usize,
) -> String {
    if items.is_empty() {
        return "当前未获取到有效的板块热点数据，请稍后重试。".to_string();
    }

    let top3: Vec<String> = items
        .iter()
        .take(3)
        .map(|s| format!("{}（{:.2}%）", s.name, s.change_pct))
        .collect();

    let mut logic_counts: HashMap<String, usize> = HashMap::new();
    for s in items {
        *logic_counts.entry(s.reason.logic.clone()).or_insert(0) += 1;
    }
    let dominant = logic_counts
        .iter()
        .max_by_key(|(_, v)| *v)
        .map(|(k, _)| k.clone())
        .unwrap_or_else(|| "普涨跟随".to_string());

    let has_fundamental = items
        .iter()
        .any(|s| s.reason.logic.contains("业绩"));
    let has_catalyst = items.iter().any(|s| s.dragon_tiger_hit);

    let mut c = format!(
        "当前市场共识别 {} 个行业、{} 个概念热点板块。热度最高的前三位为：{}。",
        industry_count, concept_count, top3.join("、")
    );
    c.push_str(&format!(
        "从上涨驱动看，以「{}」为主；",
        dominant
    ));
    if has_fundamental {
        c.push_str("其中部分板块由领涨股业绩高增支撑，具备基本面逻辑，持续性相对更强；");
    } else {
        c.push_str("多数热点暂未看到明确的业绩支撑，更多由资金与题材推动，注意追高风险；");
    }
    if has_catalyst {
        c.push_str("部分领涨股登上龙虎榜，存在机构或游资席位催化，短线波动可能加大。");
    } else {
        c.push_str("当日龙虎榜未见相关领涨股上榜，短线催化信号相对有限。");
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用板块排行
    ///
    /// 传进来的 `leader` 是**代码**，名称另造一个。刻意把两者区分开：
    /// 若哪天又有人拿名称去查财报/匹配龙虎榜，下面的单测会立刻失败。
    fn rank(code: &str, name: &str, stype: &str, change: f64, leader: &str, up: u32, down: u32) -> SectorRank {
        SectorRank {
            code: code.to_string(),
            name: name.to_string(),
            sector_type: stype.to_string(),
            change_pct: change,
            leader_stock: Some(format!("股票{}", leader)),
            leader_code: Some(leader.to_string()),
            leader_pct: Some(change + 1.0),
            net_amount: None,
            up_count: Some(up),
            down_count: Some(down),
        }
    }

    fn flow(code: &str, name: &str, inflow: f64, ratio: f64) -> SectorCapitalFlow {
        SectorCapitalFlow {
            code: code.to_string(),
            name: name.to_string(),
            main_net_inflow: inflow,
            main_net_ratio: ratio,
            huge_net_inflow: 0.0,
            large_net_inflow: 0.0,
            medium_net_inflow: 0.0,
            small_net_inflow: 0.0,
            pct_change: 0.0,
            stock_count: None,
        }
    }

    #[test]
    fn test_score_orders_by_strength() {
        let industry = vec![
            rank("BK01", "半导体", "行业", 5.0, "600001", 40, 5),
            rank("BK02", "银行", "行业", 0.5, "600002", 10, 20),
        ];
        let concept = vec![rank("BK03", "人工智能", "概念", 4.0, "300001", 30, 10)];
        let flows = vec![
            flow("BK01", "半导体", 3e9, 8.0),
            flow("BK02", "银行", 1e8, 1.0),
            flow("BK03", "人工智能", 2e9, 6.0),
        ];
        let seeds = score_hot_sectors(&industry, &concept, &flows, 3);
        assert_eq!(seeds.len(), 3);
        // 半导体：高涨幅 + 大资金流入 + 高广度 -> 应排第一
        assert_eq!(seeds[0].code, "BK01");
    }

    #[test]
    fn test_analyze_performance_driven() {
        let seeds = vec![HotSectorSeed {
            code: "BK01".to_string(),
            name: "半导体".to_string(),
            sector_type: "行业".to_string(),
            change_pct: 5.0,
            // 名称与代码刻意不同：只有按代码查才能命中下方的 financials
            leader_stock: Some("测试半导体".to_string()),
            leader_code: Some("600001".to_string()),
            leader_pct: Some(6.0),
            up_count: 40,
            down_count: 5,
            main_net_inflow: 3e9,
            main_net_ratio: 8.0,
            strength_score: 90.0,
        }];
        let mut fin = FinancialSummary {
            stock_code: "600001".to_string(),
            stock_name: "测试半导体".to_string(),
            latest_report_date: "2026-03-31".to_string(),
            pe: None,
            pb: None,
            ps: None,
            total_market_cap: None,
            float_market_cap: None,
            indicators: vec![],
        };
        fin.indicators.push(appcore::FinancialIndicator {
            report_date: "2026-03-31".to_string(),
            report_type: Some("一季报".to_string()),
            eps: None,
            bvps: None,
            cfps: None,
            roe: Some(15.0),
            roa: None,
            gross_margin: Some(45.0),
            net_margin: None,
            revenue: None,
            revenue_yoy: Some(40.0),
            net_profit: None,
            net_profit_yoy: Some(60.0),
            deducted_profit: None,
            deducted_profit_yoy: None,
            debt_ratio: None,
            current_ratio: None,
            quick_ratio: None,
        });
        let mut map = HashMap::new();
        map.insert("600001".to_string(), fin);

        let a = analyze_sector_hotspot(&seeds, &map, &[], 1, 0);
        assert_eq!(a.hot_sectors.len(), 1);
        assert_eq!(a.hot_sectors[0].reason.logic, "业绩驱动");
        assert!(a.hot_sectors[0]
            .reason
            .fundamental
            .contains("业绩高增"));
    }

    #[test]
    fn test_analyze_no_fundamental_falls_back() {
        let seeds = vec![HotSectorSeed {
            code: "BK09".to_string(),
            name: "题材X".to_string(),
            sector_type: "概念".to_string(),
            change_pct: 3.0,
            leader_stock: Some("题材X领涨".to_string()),
            leader_code: Some("300099".to_string()),
            leader_pct: Some(5.0),
            up_count: 8,
            down_count: 12,
            main_net_inflow: 5e8,
            main_net_ratio: 3.0,
            strength_score: 60.0,
        }];
        let map = HashMap::new();
        let a = analyze_sector_hotspot(&seeds, &map, &[], 0, 1);
        assert!(a.hot_sectors[0]
            .reason
            .fundamental
            .contains("未能获取领涨股基本面数据"));
        assert!(a.hot_sectors[0].leader_fundamental.is_none());
    }

    /// 领涨股基本面必须按**代码**查，不能按名称
    ///
    /// 历史故障：`leader_stock` 装的是名称（东财 f128），被直接拿去当代码查财报，
    /// 上游按 `SECURITY_CODE="芒果超媒"` 过滤必然 0 条，`leader_fundamental`
    /// 恒为 None，「业绩驱动」这条判定逻辑从未生效过。
    #[test]
    fn test_leader_fundamental_looked_up_by_code_not_name() {
        let seed = |code: Option<&str>, name: Option<&str>| HotSectorSeed {
            code: "BK01".to_string(),
            name: "半导体".to_string(),
            sector_type: "行业".to_string(),
            change_pct: 5.0,
            leader_stock: name.map(|s| s.to_string()),
            leader_code: code.map(|s| s.to_string()),
            leader_pct: Some(6.0),
            up_count: 40,
            down_count: 5,
            main_net_inflow: 3e9,
            main_net_ratio: 8.0,
            strength_score: 90.0,
        };

        let fin = FinancialSummary {
            stock_code: "600001".to_string(),
            stock_name: "测试半导体".to_string(),
            latest_report_date: "2026-03-31".to_string(),
            pe: None,
            pb: None,
            ps: None,
            total_market_cap: None,
            float_market_cap: None,
            indicators: vec![appcore::FinancialIndicator {
                report_date: "2026-03-31".to_string(),
                report_type: Some("一季报".to_string()),
                eps: None,
                bvps: None,
                cfps: None,
                roe: Some(15.0),
                roa: None,
                gross_margin: Some(45.0),
                net_margin: None,
                revenue: None,
                revenue_yoy: Some(40.0),
                net_profit: None,
                net_profit_yoy: Some(60.0),
                deducted_profit: None,
                deducted_profit_yoy: None,
                debt_ratio: None,
                current_ratio: None,
                quick_ratio: None,
            }],
        };
        let mut map = HashMap::new();
        map.insert("600001".to_string(), fin);

        // 有代码 → 命中
        let a = analyze_sector_hotspot(&[seed(Some("600001"), Some("测试半导体"))], &map, &[], 1, 0);
        assert!(
            a.hot_sectors[0].leader_fundamental.is_some(),
            "有 leader_code 时应能查到基本面"
        );

        // 只有名称、没有代码 → 查不到（对照，防止哪天又退回用名称查）
        let b = analyze_sector_hotspot(&[seed(None, Some("600001"))], &map, &[], 1, 0);
        assert!(
            b.hot_sectors[0].leader_fundamental.is_none(),
            "只有名称没有代码时必须查不到，否则说明又拿名称当代码用了"
        );
    }

    /// 龙虎榜命中必须按**代码**匹配
    #[test]
    fn test_dragon_tiger_hit_matches_by_code() {
        let seed = |code: Option<&str>, name: Option<&str>| HotSectorSeed {
            code: "BK01".to_string(),
            name: "半导体".to_string(),
            sector_type: "行业".to_string(),
            change_pct: 5.0,
            leader_stock: name.map(|s| s.to_string()),
            leader_code: code.map(|s| s.to_string()),
            leader_pct: Some(6.0),
            up_count: 40,
            down_count: 5,
            main_net_inflow: 3e9,
            main_net_ratio: 8.0,
            strength_score: 90.0,
        };
        let dt = vec![DragonTigerEntry {
            code: "600001".to_string(),
            name: "测试半导体".to_string(),
            date: "2026-08-31".to_string(),
            close_price: 10.0,
            pct_change: 6.0,
            net_amount: 1e8,
            total_buy: 2e8,
            total_sell: 1e8,
            turnover: 5.0,
            amount: 3e8,
            lhb_amount_ratio: 0.3,
            reason: "日涨幅偏离值达7%".to_string(),
            buy_seats: vec![],
            sell_seats: vec![],
            institution_net_buy: 0.0,
            hot_money_tags: vec![],
            amplitude: 0.0,
        }];

        // 代码对得上 → 命中
        let a = analyze_sector_hotspot(&[seed(Some("600001"), Some("测试半导体"))], &HashMap::new(), &dt, 1, 0);
        assert!(a.hot_sectors[0].dragon_tiger_hit, "代码在榜时应命中");

        // 只有名称 → 不命中
        let b = analyze_sector_hotspot(&[seed(None, Some("600001"))], &HashMap::new(), &dt, 1, 0);
        assert!(!b.hot_sectors[0].dragon_tiger_hit, "拿名称比对时必须不命中");
    }
}
