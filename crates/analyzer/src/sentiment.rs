use appcore::models::{
    DragonTigerEntry, HotTheme, IndustryBoard, LhbSummary, ReasonStat, ThemeStock, ThemeSummary,
};
use std::cmp::Ordering;
use std::collections::BTreeMap;

/// 个股日线（用于行业聚合，金额单位：万元）
#[derive(Debug, Clone)]
pub struct DailyBar {
    pub code: String,
    pub name: String,
    pub close: f64,
    /// 涨跌幅 (%)
    pub pct_change: f64,
    /// 成交额（万元）。腾讯日线接口不含成交额，历史交易日为 `None`
    pub amount: Option<f64>,
}

/// f64 降序比较，NaN 视为相等，避免 sort_by 内 unwrap  panic
fn desc(a: &f64, b: &f64) -> Ordering {
    b.partial_cmp(a).unwrap_or(Ordering::Equal)
}

/// 题材热点汇总：把每只强势股的 `reason` 按「+」拆开做词频统计
///
/// 同花顺的 reason 形如 `数据中心+干式变压器+电网设备+控制权变更预期`，
/// 一个标签算一次出现；同一只股票命中多个题材时，在每个题材下各计一次。
pub fn build_theme_summary(stocks: Vec<ThemeStock>) -> ThemeSummary {
    let mut grouped: BTreeMap<String, Vec<ThemeStock>> = BTreeMap::new();
    let mut tag_total: u32 = 0;

    for s in &stocks {
        for raw in s.reason.split('+') {
            let tag = raw.trim();
            if tag.is_empty() {
                continue;
            }
            tag_total += 1;
            grouped
                .entry(tag.to_string())
                .or_default()
                .push(s.clone());
        }
    }

    let mut themes: Vec<HotTheme> = grouped
        .into_iter()
        .map(|(tag, mut ss)| {
            ss.sort_by(|a, b| desc(&a.pct_change, &b.pct_change));
            let count = ss.len() as u32;
            let avg_pct = if count > 0 {
                ss.iter().map(|x| x.pct_change).sum::<f64>() / count as f64
            } else {
                0.0
            };
            let leader = ss.first().cloned();
            HotTheme {
                tag,
                count,
                leader: leader.as_ref().map(|x| x.name.clone()),
                leader_pct: leader.as_ref().map(|x| x.pct_change).unwrap_or(0.0),
                avg_pct,
                stocks: ss,
            }
        })
        .collect();

    // 先按命中只数降序，只数相同再按题材内平均涨幅降序
    themes.sort_by(|a, b| b.count.cmp(&a.count).then(desc(&a.avg_pct, &b.avg_pct)));

    let top_tag = themes.first().map(|t| t.tag.clone());

    ThemeSummary {
        stock_count: stocks.len() as u32,
        tag_total,
        theme_total: themes.len() as u32,
        themes,
        top_tag,
    }
}

/// 行业聚合结果（内部中间态）
struct BoardAcc {
    pct_sum: f64,
    /// 成交额累加。全部成分股都无成交额时为 None
    amount_sum: Option<f64>,
    up: u32,
    down: u32,
    flat: u32,
    count: u32,
    leader: Option<(String, String, f64)>,
}

/// 按行业聚合成分股日线，得到行业涨跌幅 / 涨跌家数 / 领涨股 / 成交额
///
/// **口径**：行业涨跌幅 = 该行业成分股当日涨跌幅的**等权算术平均**，
/// 而非东财板块指数的官方涨跌幅（东财未提供历史日期的板块指数接口）。
/// 东财官方成分股家数通过 `member_counts` 单独透出，便于对照。
pub fn aggregate_boards(
    bars: &[DailyBar],
    industry_of: &std::collections::HashMap<String, String>,
    member_counts: &std::collections::HashMap<String, u32>,
) -> Vec<IndustryBoard> {
    let mut acc: BTreeMap<String, BoardAcc> = BTreeMap::new();

    for bar in bars {
        let Some(ind) = industry_of.get(&bar.code) else {
            continue;
        };
        let e = acc.entry(ind.clone()).or_insert(BoardAcc {
            pct_sum: 0.0,
            amount_sum: None,
            up: 0,
            down: 0,
            flat: 0,
            count: 0,
            leader: None,
        });

        e.pct_sum += bar.pct_change;
        if let Some(a) = bar.amount {
            *e.amount_sum.get_or_insert(0.0) += a;
        }
        e.count += 1;

        if bar.pct_change > 0.0 {
            e.up += 1;
        } else if bar.pct_change < 0.0 {
            e.down += 1;
        } else {
            e.flat += 1;
        }

        // 领涨股取行业内涨幅最大的一只
        let better = match &e.leader {
            None => true,
            Some((_, _, best)) => bar.pct_change > *best,
        };
        if better {
            e.leader = Some((bar.code.clone(), bar.name.clone(), bar.pct_change));
        }
    }

    let mut boards: Vec<IndustryBoard> = acc
        .into_iter()
        .map(|(name, e)| {
            let change_pct = if e.count > 0 {
                e.pct_sum / e.count as f64
            } else {
                0.0
            };
            let (leader_code, leader, leader_pct) = match e.leader {
                Some((c, n, p)) => (Some(c), Some(n), p),
                None => (None, None, 0.0),
            };
            IndustryBoard {
                name: name.clone(),
                change_pct,
                up_count: e.up,
                down_count: e.down,
                flat_count: e.flat,
                leader,
                leader_code,
                leader_pct,
                amount: e.amount_sum,
                member_count: member_counts.get(&name).copied().unwrap_or(0),
                calc_count: e.count,
            }
        })
        .collect();

    boards.sort_by(|a, b| desc(&a.change_pct, &b.change_pct));
    boards
}

/// 用东财板块快照构建行业榜（快路径）
///
/// 与 `aggregate_boards` 的区别：涨跌幅、成交额、涨跌家数全部是**东财官方口径**，
/// 不做成分股等权聚合，也不需要抓任何个股日线。
///
/// `member_counts` 提供官方成分股家数（来自 `RPT_VALUEINDUSTRY_DET`）。
///
/// `industry_universe` 是**过滤用的行业全集**，应当与慢路径 `aggregate_boards` 所依赖
/// 的个行归属集合同源（本库 `stock_industry` 的 distinct industry）。
///
/// **为什么不能直接用 `member_counts` 当过滤器**：它只有官方清单那 127 个行业，
/// 而慢路径是按个股行业归属聚合的，实测多出「体育Ⅱ」——该行业有 3 只真实成分股、
/// 东财板块快照里也有（BK1273），只是不在官方清单内。若用 `member_counts` 过滤，
/// 快路径会静默少一个行业，两条路径的行业榜行数对不上。
///
/// `industry_universe` 传 `None` 或空集时退回用 `member_counts` 过滤，用于冷启动
/// （行业归属表尚未落库，此时不值得为对齐口径去触发一次全市场行业抓取）。
pub fn boards_from_snapshots(
    snapshots: &[fetcher::BoardSnapshot],
    member_counts: &std::collections::HashMap<String, u32>,
    industry_universe: Option<&std::collections::HashSet<String>>,
) -> Vec<IndustryBoard> {
    let universe = industry_universe.filter(|s| !s.is_empty());

    let mut boards: Vec<IndustryBoard> = snapshots
        .iter()
        .filter(|s| match universe {
            Some(u) => u.contains(&s.name),
            None => member_counts.contains_key(&s.name),
        })
        .map(|s| IndustryBoard {
            name: s.name.clone(),
            change_pct: s.change_pct,
            up_count: s.up_count,
            down_count: s.down_count,
            flat_count: s.flat_count,
            leader: s.leader.clone(),
            leader_code: s.leader_code.clone(),
            leader_pct: s.leader_pct,
            amount: Some(s.amount),
            // 官方清单外的行业（如体育Ⅱ）拿不到官方成分家数 → 0，渲染层显示「—」
            member_count: member_counts.get(&s.name).copied().unwrap_or(0),
            // 官方口径下没有"实际参与计算的家数"这一说，用涨跌平之和回填
            calc_count: s.up_count + s.down_count + s.flat_count,
        })
        .collect();

    boards.sort_by(|a, b| desc(&a.change_pct, &b.change_pct));
    boards
}

/// 龙虎榜汇总
///
/// **口径要点**：同一只股票可能触发多个上榜原因，东财会返回多条记录，且各记录的
/// `net_amount` 可能不同。因此同时输出两套合计：
/// - `net_total`：按个股去重后的合计（去重时取该股净买入最大的那条记录）
/// - `records_sum`：所有记录直接求和
///
/// 明细仍保留全部记录，前端对重复个股做标记。
pub fn summarize_lhb(
    mut entries: Vec<DragonTigerEntry>,
    reason_top_n: usize,
) -> LhbSummary {
    entries.sort_by(|a, b| desc(&a.net_amount, &b.net_amount));

    // entries 已按净买入降序，同一 code 首次出现的即为该股最大净买入记录
    let mut seen: std::collections::HashMap<String, &DragonTigerEntry> =
        std::collections::HashMap::new();
    let mut reason_counter: BTreeMap<String, u32> = BTreeMap::new();

    for e in &entries {
        seen.entry(e.code.clone()).or_insert(e);
        if !e.reason.is_empty() {
            *reason_counter.entry(e.reason.clone()).or_insert(0) += 1;
        }
    }

    let stock_count = seen.len() as u32;
    let record_count = entries.len() as u32;

    let net_total: f64 = seen.values().map(|e| e.net_amount).sum();
    let buy_total: f64 = seen.values().map(|e| e.total_buy).sum();
    let sell_total: f64 = seen.values().map(|e| e.total_sell).sum();
    let amount_total: f64 = seen.values().map(|e| e.amount).sum();

    let net_buy_count = seen.values().filter(|e| e.net_amount > 0.0).count() as u32;
    let net_sell_count = seen.values().filter(|e| e.net_amount < 0.0).count() as u32;

    let records_sum: f64 = entries.iter().map(|e| e.net_amount).sum();

    let mut reasons: Vec<ReasonStat> = reason_counter
        .into_iter()
        .map(|(reason, count)| ReasonStat { reason, count })
        .collect();
    reasons.sort_by(|a, b| b.count.cmp(&a.count));
    reasons.truncate(reason_top_n);

    LhbSummary {
        stock_count,
        record_count,
        net_total,
        records_sum,
        buy_total,
        sell_total,
        net_buy_count,
        net_sell_count,
        amount_total,
        entries,
        top_reasons: reasons,
    }
}

/// 全市场涨跌家数与总成交额
///
/// 总成交额在所有成分股均无成交额时为 `None`（历史交易日的常见情况）。
pub fn calc_breadth(bars: &[DailyBar]) -> (u32, u32, u32, u32, Option<f64>) {
    let mut up = 0u32;
    let mut down = 0u32;
    let mut flat = 0u32;
    let mut amount: Option<f64> = None;

    for b in bars {
        if b.pct_change > 0.0 {
            up += 1;
        } else if b.pct_change < 0.0 {
            down += 1;
        } else {
            flat += 1;
        }
        if let Some(a) = b.amount {
            *amount.get_or_insert(0.0) += a;
        }
    }

    (up, down, flat, bars.len() as u32, amount)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    fn snap(name: &str, pct: f64, up: u32, down: u32, flat: u32) -> fetcher::BoardSnapshot {
        fetcher::BoardSnapshot {
            name: name.to_string(),
            bk_code: "BK0000".to_string(),
            change_pct: pct,
            amount: 100.0,
            up_count: up,
            down_count: down,
            flat_count: flat,
            leader: Some("领涨股".to_string()),
            leader_code: Some("000001".to_string()),
            leader_pct: pct + 1.0,
        }
    }

    #[test]
    fn test_boards_from_snapshots_filters_to_industry_list() {
        // 板块全集（含一级/三级行业）必须收敛到东财二级行业，
        // 否则快路径与慢路径输出的行业榜行数会不一致。
        // 传 None = 冷启动（行业归属表未落库），此时退回用官方清单过滤。
        let mut members = HashMap::new();
        members.insert("电力".to_string(), 80u32);
        members.insert("半导体".to_string(), 200u32);

        let snaps = vec![
            snap("电力", 1.5, 50, 30, 0),
            snap("某三级行业", 9.9, 1, 0, 0),
            snap("半导体", -2.0, 20, 170, 10),
        ];
        let boards = boards_from_snapshots(&snaps, &members, None);

        assert_eq!(boards.len(), 2);
        let names: Vec<&str> = boards.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, vec!["电力", "半导体"]);
        // 官方口径：涨跌幅直接透传，不做等权聚合
        assert_eq!(boards[0].change_pct, 1.5);
        assert_eq!(boards[0].member_count, 80);
        // calc_count 用涨跌平之和回填
        assert_eq!(boards[0].calc_count, 80);
        assert_eq!(boards[1].up_count, 20);
        assert_eq!(boards[1].flat_count, 10);
        // 快路径一定有成交额
        assert_eq!(boards[0].amount, Some(100.0));
    }

    #[test]
    fn test_boards_from_snapshots_empty_on_no_overlap() {
        let snaps = vec![snap("电力", 1.5, 1, 0, 0)];
        let members: HashMap<String, u32> = HashMap::new();
        assert!(boards_from_snapshots(&snaps, &members, None).is_empty());
    }

    #[test]
    fn test_boards_from_snapshots_universe_includes_unofficial_industry() {
        // 真实 bug 场景：体育Ⅱ 有 3 只真实成分股、板块快照里也有（BK1273），
        // 但不在官方 RPT_VALUEINDUSTRY_DET 的 127 个二级行业清单里。
        // 若用 member_counts 过滤，快路径会静默少掉这个行业（127 vs 慢路径 128）。
        let mut members = HashMap::new();
        members.insert("电力".to_string(), 80u32);
        members.insert("半导体".to_string(), 200u32);

        // 行业全集与慢路径同源，比官方清单多一个体育Ⅱ
        let universe: HashSet<String> = ["电力", "半导体", "体育Ⅱ"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let snaps = vec![
            snap("电力", 1.5, 50, 30, 0),
            snap("体育Ⅱ", 2.17, 2, 1, 0),
            snap("某三级行业", 9.9, 1, 0, 0),
            snap("半导体", -2.0, 20, 170, 10),
        ];
        let boards = boards_from_snapshots(&snaps, &members, Some(&universe));

        assert_eq!(boards.len(), 3, "三级行业应被排除，体育Ⅱ 应保留");
        let names: Vec<&str> = boards.iter().map(|b| b.name.as_str()).collect();
        // 按涨跌幅降序
        assert_eq!(names, vec!["体育Ⅱ", "电力", "半导体"]);

        // 官方清单外的行业拿不到官方成分家数 → 0，渲染层显示「—」
        let tiyu = &boards[0];
        assert_eq!(tiyu.member_count, 0);
        assert_eq!(tiyu.calc_count, 3);
        assert_eq!(tiyu.change_pct, 2.17);
        // 官方清单内的行业照常拿到成分家数
        assert_eq!(boards[1].member_count, 80);
    }

    #[test]
    fn test_boards_from_snapshots_empty_universe_falls_back_to_member_counts() {
        // 空集与 None 等价：都退回官方清单过滤，避免冷启动时过滤出 0 个行业
        let mut members = HashMap::new();
        members.insert("电力".to_string(), 80u32);

        let snaps = vec![snap("电力", 1.5, 1, 0, 0), snap("某三级行业", 9.9, 1, 0, 0)];

        let empty: HashSet<String> = HashSet::new();
        let a = boards_from_snapshots(&snaps, &members, Some(&empty));
        let b = boards_from_snapshots(&snaps, &members, None);

        assert_eq!(a.len(), 1);
        assert_eq!(a[0].name, "电力");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].name, "电力");
    }

    fn stock(code: &str, name: &str, pct: f64, reason: &str) -> ThemeStock {
        ThemeStock {
            code: code.to_string(),
            name: name.to_string(),
            pct_change: pct,
            reason: reason.to_string(),
        }
    }

    #[test]
    fn test_theme_split_by_plus() {
        let stocks = vec![
            stock("000001", "甲", 10.0, "数据中心+电网设备"),
            stock("000002", "乙", 9.0, "数据中心+储能"),
            stock("000003", "丙", 8.0, "电网设备"),
        ];
        let s = build_theme_summary(stocks);

        assert_eq!(s.stock_count, 3);
        // 3 只股票共 5 个标签
        assert_eq!(s.tag_total, 5);
        // 去重后 3 个题材
        assert_eq!(s.theme_total, 3);
        // 数据中心命中 2 只，排第一
        assert_eq!(s.top_tag.as_deref(), Some("数据中心"));
        assert_eq!(s.themes[0].count, 2);
        assert_eq!(s.themes[0].leader.as_deref(), Some("甲"));
    }

    #[test]
    fn test_board_aggregate_equal_weight() {
        let bars = vec![
            DailyBar { code: "1".into(), name: "A".into(), close: 10.0, pct_change: 10.0, amount: Some(100.0) },
            DailyBar { code: "2".into(), name: "B".into(), close: 10.0, pct_change: 0.0, amount: Some(100.0) },
            DailyBar { code: "3".into(), name: "C".into(), close: 10.0, pct_change: -5.0, amount: Some(100.0) },
        ];
        let mut ind = std::collections::HashMap::new();
        ind.insert("1".to_string(), "半导体".to_string());
        ind.insert("2".to_string(), "半导体".to_string());
        ind.insert("3".to_string(), "银行".to_string());

        let mut mc = std::collections::HashMap::new();
        mc.insert("半导体".to_string(), 2u32);

        let boards = aggregate_boards(&bars, &ind, &mc);
        // 半导体涨 1 跌 0 平 1，等权均值 (10 + 0)/2 = 5.0
        let semi = boards.iter().find(|b| b.name == "半导体").unwrap();
        assert!((semi.change_pct - 5.0).abs() < 1e-9);
        assert_eq!(semi.up_count, 1);
        assert_eq!(semi.flat_count, 1);
        assert_eq!(semi.member_count, 2);
        assert_eq!(semi.calc_count, 2);
        assert_eq!(semi.amount, Some(200.0));
        // 领涨股为涨幅最大的 A
        assert_eq!(semi.leader.as_deref(), Some("A"));
    }

    #[test]
    fn test_board_amount_none_when_all_missing() {
        // 历史交易日：腾讯日线无成交额，全部为 None，则板块成交额应为 None 而非 0
        let bars = vec![
            DailyBar { code: "1".into(), name: "A".into(), close: 10.0, pct_change: 3.0, amount: None },
            DailyBar { code: "2".into(), name: "B".into(), close: 10.0, pct_change: 1.0, amount: None },
        ];
        let mut ind = std::collections::HashMap::new();
        ind.insert("1".to_string(), "半导体".to_string());
        ind.insert("2".to_string(), "半导体".to_string());
        let empty = std::collections::HashMap::new();

        let boards = aggregate_boards(&bars, &ind, &empty);
        assert_eq!(boards[0].amount, None);
        // 涨跌幅仍应正常计算
        assert!((boards[0].change_pct - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_lhb_dedup_keeps_max_net_buy() {
        let mk = |code: &str, net: f64, reason: &str| DragonTigerEntry {
            code: code.to_string(),
            name: format!("股票{}", code),
            date: "2026-07-24".to_string(),
            close_price: 10.0,
            pct_change: 5.0,
            turnover: 1.0,
            amount: 1000.0,
            amplitude: 0.0,
            net_amount: net,
            total_buy: net + 100.0,
            total_sell: 100.0,
            lhb_amount_ratio: 1.0,
            reason: reason.to_string(),
            buy_seats: vec![],
            sell_seats: vec![],
            institution_net_buy: 0.0,
            hot_money_tags: vec![],
        };

        let entries = vec![
            mk("002083", 16370.0, "连续三日涨幅偏离"),
            mk("002083", 13920.0, "日涨幅偏离7%"),
            mk("601678", 17800.0, "换手率达20%"),
        ];
        let s = summarize_lhb(entries, 8);

        assert_eq!(s.stock_count, 2);
        assert_eq!(s.record_count, 3);
        // 去重取最大：17800 + 16370 = 34170
        assert!((s.net_total - 34170.0).abs() < 1e-6);
        // 记录直接求和：17800 + 16370 + 13920 = 48090
        assert!((s.records_sum - 48090.0).abs() < 1e-6);
        assert_eq!(s.net_buy_count, 2);
        assert_eq!(s.net_sell_count, 0);
        // 明细保留全部 3 条
        assert_eq!(s.entries.len(), 3);
    }

    #[test]
    fn test_breadth_counts() {
        let bars = vec![
            DailyBar { code: "1".into(), name: "A".into(), close: 1.0, pct_change: 1.0, amount: Some(10.0) },
            DailyBar { code: "2".into(), name: "B".into(), close: 1.0, pct_change: -1.0, amount: Some(20.0) },
            DailyBar { code: "3".into(), name: "C".into(), close: 1.0, pct_change: 0.0, amount: Some(30.0) },
        ];
        let (up, down, flat, covered, amount) = calc_breadth(&bars);
        assert_eq!((up, down, flat, covered), (1, 1, 1, 3));
        assert_eq!(amount, Some(60.0));
    }

    #[test]
    fn test_breadth_amount_none_when_all_missing() {
        let bars = vec![
            DailyBar { code: "1".into(), name: "A".into(), close: 1.0, pct_change: 1.0, amount: None },
            DailyBar { code: "2".into(), name: "B".into(), close: 1.0, pct_change: -1.0, amount: None },
        ];
        let (up, down, _, covered, amount) = calc_breadth(&bars);
        assert_eq!((up, down, covered), (1, 1, 2));
        assert_eq!(amount, None);
    }
}
