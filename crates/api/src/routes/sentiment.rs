use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use futures::stream::{self, StreamExt};
use serde::Serialize;

use analyzer::sentiment::{
    aggregate_boards, boards_from_snapshots, build_theme_summary, calc_breadth, summarize_lhb,
    DailyBar,
};
use appcore::models::{
    Kline, KlinePeriod, SentimentArchiveEntry, SentimentArchiveList, SentimentDaily,
    SentimentDatesQuery, SentimentQuery,
};
use fetcher::{EastmoneyFetcher, KlineStatus, ThsFetcher};

use crate::state::AppState;

/// 明细表最多展示的记录数（龙虎榜）
const LHB_REASON_TOP_N: usize = 8;
/// 并发抓取日线的任务数。
///
/// ## 2026-09-03 逐档扫描（2026-08-27 冷启动，5642 只，交替顺序各 3 轮）
///
/// | 并发 | 中位耗时 | covered | 单请求延迟 | 干净率 |
/// |---|---|---|---|---|
/// | 24 | 23186ms | 5624 | 99ms | 3/3 |
/// | **48** | **12677ms** | **5624** | 108ms | **6/6** |
/// | 96 | ~8779ms | 5624 ×8、**5398 ×1** | 139ms | 8/9 |
/// | 128 | 2225ms | **316 / 567 / 350** | — | 0/3 |
/// | 192 | 2608ms | **187 / 120 / 184** | — | 0/3 |
///
/// ## 为什么是 48 而不是更快的 96
///
/// **128/192 的耗时是假的**：它们只有 2 秒多，是因为几乎全部请求失败，`covered`
/// 从 5624 崩到一两百——上游被我们打挂了。只盯耗时就会把服务改成产出垃圾数据，
/// 所以判据必须带上覆盖数：只要 `covered` 掉了，再快也不算数。
///
/// 96 比 48 快约 4 秒，但 9 轮里有一轮丢了 226 只股票，且它离 128 的悬崖只有
/// 1.33 倍余量；48 是 6/6 全净、距悬崖 2.7 倍。少 4 秒换「报告偶尔少两百只」，
/// 不划算——慢路径的产出是要直接给人看的。
///
/// ## 两个干扰项，改之前先知道
///
/// 1. **顺序是干扰项。** 早先按 8→16→24→32 顺序测，结论是「8 最快」；改成
///    24→8→24→8 交替后结论反转。新浪会随时间累积限流状态，先跑的天然占便宜。
/// 2. **连测会污染后续批次。** 连跑 15+ 次全量抓取后，96 的耗时从 7.9s 逐轮涨到
///    11.2s——上游在累积限流。跨批次的数字不可直接横比，只有同批交替才有意义。
///
/// 结论：48 是当前上游状态下的安全上限，不要凭单次观测往上调。
const BAR_CONCURRENCY: usize = 48;

/// 快路径抓取失败后的重试间隔
const FAST_RETRY_DELAY: Duration = Duration::from_millis(1500);

/// 日线序列缓存的固定回溯窗口（根）
///
/// 两个行情源（腾讯 / 新浪）都只支持「最近 N 根」，不支持按日期区间查询，
/// 所以查一个历史交易日原本要把全市场 5600+ 只重抓一遍（约 13s）。
///
/// 但实测**多取几根几乎不要钱**：新浪单请求延迟 20 根 241ms → 120 根 242ms；
/// 端到端冷启动 20 根 vs 62 根交替三轮，中位 14362ms vs 12389ms——没有差异。
/// （早先单次观测到 21s，让我以为「62 根慢 65%」；交替重测后结论反转，那 21s
/// 是上游当时在限流。单次观测不能当结论。）
///
/// 于是策略从「刚好抓够目标日那几根」改成「固定抓半年、整条序列落库」：窗口内
/// 的其它日期之后都是 0 请求。120 根 ≈ 半年；更早的历史日期由 `bars_needed_for`
/// 兜住（两者取大值）。
///
/// 代价：单只股票的写入量从 1 行变成约 120 行，全量冷启动一次约 67 万行。这也是
/// `storage::save_daily_bars` 必须合成多行 INSERT 的原因。
const SERIES_WINDOW_BARS: u32 = 120;

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

type ApiError = (StatusCode, Json<ErrorResponse>);

fn err(status: StatusCode, msg: impl Into<String>) -> ApiError {
    let msg = msg.into();
    tracing::error!(%msg, "情绪日报请求失败");
    (status, Json(ErrorResponse { error: msg }))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/v1/sentiment/daily",
            get(get_daily).delete(delete_daily),
        )
        .route("/api/v1/sentiment/daily/export", get(export_daily))
        .route("/api/v1/sentiment/dates", get(list_dates))
}

/// GET /api/v1/sentiment/daily - 获取指定交易日的市场情绪日报
async fn get_daily(
    State(state): State<Arc<AppState>>,
    Query(q): Query<SentimentQuery>,
) -> Result<Json<SentimentDaily>, ApiError> {
    let date = resolve_date( q.date.as_deref()).await?;
    let repo = storage::Repository::from_database(&state.db);

    // 命中缓存直接返回，避免每次都重跑数千次日线抓取
    if !q.refresh.unwrap_or(false) {
        if let Some((payload, _)) = repo
            .get_sentiment_daily(&date)
            .await
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        {
            match serde_json::from_str::<SentimentDaily>(&payload) {
                Ok(mut cached) => {
                    cached.methodology.from_cache = true;
                    cached.methodology.elapsed_ms = 0;
                    return Ok(Json(cached));
                }
                Err(e) => {
                    // 缓存结构变更时丢弃旧数据重算，不因缓存损坏而让接口不可用
                    tracing::warn!(error = %e, "缓存的日报反序列化失败，将重新计算");
                }
            }
        }
    }

    // 标记该日期正在生成：定时任务据此避让，避免两轮全市场抓取撞在一起被限流
    let _guard = state.begin_date(&date);
    let report = build_daily(&state, &repo, &date)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 残缺日报不落缓存：上游限流会把全市场日线抓成空，此时宽度与行业全是 0。
    // 若把它写进缓存，后续请求会一直拿到这份空报告，只能手工 refresh 才能恢复。
    if is_degraded(&report) {
        tracing::warn!(
            date = %report.date,
            covered = report.breadth.covered_count,
            boards = report.boards.len(),
            "日报数据不完整（疑似上游限流），本次结果不写入缓存"
        );
        return Ok(Json(report));
    }

    cache_if_complete(&repo, &report).await;

    Ok(Json(report))
}

/// 日报是否残缺（缺成分股日线或行业聚合结果）
///
/// 正常情况下全市场至少覆盖数千只个股、上百个行业；全为 0 只可能是上游
/// 限流返回空响应，不是真实的市场状态。
pub(crate) fn is_degraded(r: &SentimentDaily) -> bool {
    r.breadth.covered_count == 0 || r.boards.is_empty()
}

/// 写入缓存，失败仅记日志不影响本次响应
pub(crate) async fn cache_if_complete(repo: &storage::Repository, r: &SentimentDaily) {
    if is_degraded(r) {
        tracing::warn!(
            date = %r.date,
            covered = r.breadth.covered_count,
            boards = r.boards.len(),
            "日报数据不完整，跳过缓存写入"
        );
        return;
    }
    if let Ok(payload) = serde_json::to_string(r) {
        if let Err(e) = repo.save_sentiment_daily(&r.date, &payload).await {
            tracing::warn!(error = %e, "日报缓存写入失败");
        }
    }
}

/// GET /api/v1/sentiment/daily/export - 导出单文件 HTML 日报（资源全内联，可离线打开）
async fn export_daily(
    State(state): State<Arc<AppState>>,
    Query(q): Query<SentimentQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let date = resolve_date( q.date.as_deref()).await?;
    let repo = storage::Repository::from_database(&state.db);

    let report = if !q.refresh.unwrap_or(false) {
        match repo.get_sentiment_daily(&date).await {
            Ok(Some((payload, _))) => serde_json::from_str::<SentimentDaily>(&payload).ok(),
            _ => None,
        }
    } else {
        None
    };

    let report = match report {
        Some(r) => r,
        None => {
            let _guard = state.begin_date(&date);
            let r = build_daily(&state, &repo, &date)
                .await
                .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            cache_if_complete(&repo, &r).await;
            r
        }
    };

    let html = crate::routes::sentiment_report::render(&report);
    // 服务端留一份：归档列表的「下载 HTML」直接取这份，不必每次重新渲染，
    // 也顺带把日报沉淀成项目内的归档库。写失败不影响本次导出——留存只是附加
    // 价值，与 cache_if_complete 的容错风格一致。
    archive_export(&state, &report.date, &html);

    let filename = format!(
        "attachment; filename=\"market-sentiment-{}.html\"",
        report.date
    );

    Ok((
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8".to_string()),
            (header::CONTENT_DISPOSITION, filename),
        ],
        Html(html),
    ))
}

/// 服务端导出件的留存路径
pub(crate) fn export_path_for(state: &AppState, date: &str) -> std::path::PathBuf {
    state.report_dir.join(format!("sentiment-{date}.html"))
}

/// 把已渲染的导出件写入服务端留存目录，返回是否落盘成功
///
/// 手动导出与自动归档共用它：自动归档顺手留一份，归档目录里那天的「下载 HTML」
/// 就直接可用，不必等用户点导出才生成（渲染实测只要 4 ms）。
pub(crate) fn archive_export(state: &AppState, date: &str, html: &str) -> bool {
    let path = export_path_for(state, date);
    match std::fs::write(&path, html) {
        Ok(()) => {
            tracing::info!(path = %path.display(), bytes = html.len(), "日报导出件已留档");
            true
        }
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "日报导出件留档失败");
            false
        }
    }
}

/// GET /api/v1/sentiment/dates - 已归档日报的目录
///
/// 缺了它，用户只能拿日期选择器盲猜哪天已经生成过——猜错就是一次约 20 秒的
/// 冷启动。实测缓存命中 0.006s、冷启动 19.8s，差三个数量级，把用户往缓存
/// 路径上引导是这块最大的收益。
async fn list_dates(
    State(state): State<Arc<AppState>>,
    Query(q): Query<SentimentDatesQuery>,
) -> Result<Json<SentimentArchiveList>, ApiError> {
    let repo = storage::Repository::from_database(&state.db);
    let mut entries: Vec<SentimentArchiveEntry> = repo
        .list_sentiment_daily(q.limit.unwrap_or(60).clamp(1, 365))
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 仓库层不碰磁盘，has_export 在这里按文件系统实况补上
    for e in &mut entries {
        e.has_export = export_path_for(&state, &e.date).exists();
    }

    // 归档是否还在自动积累，前端要有地方看得到。查不到记录说明从未跑过
    // （自动归档未开启），不是错误。
    let last_run = repo
        .latest_job_run(crate::jobs::JOB_SENTIMENT_ARCHIVE)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "查询自动归档运行记录失败");
            None
        });

    Ok(Json(SentimentArchiveList {
        total: entries.len(),
        entries,
        last_run,
    }))
}

/// DELETE /api/v1/sentiment/daily?date=YYYY-MM-DD - 删除某交易日的日报归档
///
/// 用于清掉坏数据后让「重新计算」落到干净状态。只删日报本身，不动
/// `daily_bar_cache`——那份成分股日线没有错，留着能让重算快得多。
async fn delete_daily(
    State(state): State<Arc<AppState>>,
    Query(q): Query<SentimentQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let raw = q
        .date
        .as_deref()
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "必须指定要删除的 date"))?;
    let date = normalize_date(raw)
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "日期格式应为 YYYY-MM-DD 或 YYYYMMDD"))?;

    let repo = storage::Repository::from_database(&state.db);
    let removed = repo
        .delete_sentiment_daily(&date)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 导出件是派生物，主数据删了它也该一起走，否则归档目录里会残留孤儿
    let p = export_path_for(&state, &date);
    if p.exists() {
        if let Err(e) = std::fs::remove_file(&p) {
            tracing::warn!(path = %p.display(), error = %e, "删除归档时清理导出件失败");
        }
    }

    Ok(Json(serde_json::json!({ "date": date, "removed": removed })))
}

/// 解析并校验交易日
///
/// 未指定日期时，从今天向前回溯找出最近的交易日。判据是东财行业板块清单
/// 在该日是否有数据——这是唯一能按历史日期查询的官方接口，可同时充当
/// 「该日是否为交易日」的权威探测。
///
/// 实测该报表**只在交易日收盘后落库**：盘中查当天返回 0 条（2026-09-01
/// 13:00 查询为 0，而 08-31、08-28 均为 127 条，周末为 0 条）。因此无需
/// 额外的「是否收盘」时间判断——探测本身就等价于「该日行情已落定」。
///
/// 不要用腾讯指数日线做这个判据：日报冷启动会对它发起数千次请求，此时
/// 该接口最可能处于限流状态，用它探测会导致连交易日都解析不出来。
async fn resolve_date(raw: Option<&str>) -> Result<String, ApiError> {
    if let Some(d) = raw {
        return normalize_date(d).ok_or_else(|| {
            err(
                StatusCode::BAD_REQUEST,
                format!("日期格式无效: {d}（应为 YYYY-MM-DD 或 YYYYMMDD）"),
            )
        });
    }

    let em = EastmoneyFetcher::new();
    let today = chrono::Local::now().date_naive();

    for back in 0..15 {
        let candidate = today - chrono::Duration::days(back);
        // 跳过周末，减少无效探测请求（%u: 1=周一 … 7=周日）
        if let Some(d) = weekday_num(&candidate.format("%Y-%m-%d").to_string()) {
            if d >= 6 {
                continue;
            }
        }
        let date = candidate.format("%Y-%m-%d").to_string();
        match em.fetch_industry_boards(&date).await {
            Ok(boards) if !boards.is_empty() => return Ok(date),
            _ => continue,
        }
    }

    Err(err(
        StatusCode::SERVICE_UNAVAILABLE,
        "无法确定最近的交易日，请显式指定 date 参数",
    ))
}

/// 归一化日期为 YYYY-MM-DD
fn normalize_date(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.len() == 10 && s.as_bytes().get(4) == Some(&b'-') {
        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .ok()
            .map(|d| d.format("%Y-%m-%d").to_string())
    } else if s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit()) {
        chrono::NaiveDate::parse_from_str(s, "%Y%m%d")
            .ok()
            .map(|d| d.format("%Y-%m-%d").to_string())
    } else {
        None
    }
}

/// 星期几，1 = 周一 … 7 = 周日
///
/// 用 `%u` 而非 `NaiveDate::weekday()`：后者在当前 chrono 版本中是私有方法。
fn weekday_num(date: &str) -> Option<u32> {
    let d = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    d.format("%u").to_string().parse::<u32>().ok()
}

/// 汇总生成一整份情绪日报
///
/// `pub(crate)`：`jobs` 模块的自动归档任务要复用它，与手动请求走完全相同的
/// 生成路径——两条路径一旦分叉，归档里的数据就可能和用户看到的不是一回事。
pub(crate) async fn build_daily(
    state: &AppState,
    repo: &storage::Repository,
    date: &str,
) -> appcore::AppResult<SentimentDaily> {
    let started = std::time::Instant::now();
    let mut notes: Vec<String> = Vec::new();

    // ── ① 龙虎榜 ──────────────────────────────────────────────
    let entries = state.fetcher.fetch_dragon_tiger(Some(date)).await?;
    let lhb = summarize_lhb(entries, LHB_REASON_TOP_N);

    // ── ② 题材热点 ────────────────────────────────────────────
    let themes = match ThsFetcher::new().fetch_hot_themes(date).await {
        Ok(stocks) => build_theme_summary(stocks),
        Err(e) => {
            tracing::warn!(error = %e, "同花顺题材数据获取失败，日报中题材部分为空");
            notes.push(format!("题材热点数据获取失败：{e}"));
            build_theme_summary(Vec::new())
        }
    };

    // ── ③ 行业轮动 ────────────────────────────────────────────
    let em = EastmoneyFetcher::new();

    // 东财官方披露的板块成分股家数
    let member_counts: HashMap<String, u32> = em
        .fetch_industry_boards(date)
        .await
        .unwrap_or_default()
        .into_iter()
        .collect();

    // 快路径要用与慢路径同源的行业全集做过滤，否则会少掉官方清单外的行业。
    // 这里只读库、不触发网络抓取：行业归属表为空（冷启动）时退回官方清单过滤，
    // 不值得为对齐一个行业的口径去发一次全市场行业抓取请求。
    let industry_universe: Option<HashSet<String>> = repo
        .get_industry_names()
        .await
        .ok()
        .filter(|v| !v.is_empty())
        .map(|v| v.into_iter().collect());

    let mut sources = vec![
        "东方财富 · 龙虎榜明细 RPT_DAILYBILLBOARD_DETAILSNEW".to_string(),
        "同花顺 · 当日强势股题材归因 getharden".to_string(),
        "东方财富 · 行业板块清单 RPT_VALUEINDUSTRY_DET".to_string(),
    ];

    // 快路径只覆盖最近一个交易日；其余日期一律回落到逐股聚合
    let (boards, bars, fast_amount, board_method, fetch_stats) =
        match try_fast_path(&em, &member_counts, industry_universe.as_ref(), date).await {
            Some((b, bars, amount)) => {
                // 板块快照可能来自东财 push2 集群，也可能在集群不可达时回落新浪
                sources.push("行业板块快照（东财 push2 多节点，不可达时回落新浪）".to_string());
                sources.push("东方财富 · 全市场个股快表 RPT_DMSK_TS_STOCKNEW".to_string());
                notes.push(format!(
                    "行业涨跌幅为东财板块官方口径，市场宽度覆盖 {} 只沪深个股；\
                     该快表不含北交所，与历史交易日的逐股聚合口径不同，两者不可直接横向比较。",
                    bars.len()
                ));
                (
                    b,
                    bars,
                    amount,
                    "东财板块官方涨跌幅".to_string(),
                    BarFetchStats::default(),
                )
            }
            None => {
                sources.push("东方财富 · 个股行业归属 RPT_F10_CORETHEME_BOARDTYPE".to_string());
                sources.push("腾讯财经 / 新浪财经 · 个股与指数日线".to_string());

                // 个股 → 行业归属（首次抓取后落库，行业归属变化缓慢，无需每次重抓）
                let industry_rows = match repo.count_stock_industries().await {
                    Ok(0) | Err(_) => {
                        let rows = em.fetch_stock_industry_map().await?;
                        repo.save_stock_industries(&rows).await.ok();
                        rows
                    }
                    Ok(_) => repo.get_industry_map().await.unwrap_or_default(),
                };

                let industry_of: HashMap<String, String> = industry_rows
                    .iter()
                    .map(|(code, _, industry)| (code.clone(), industry.clone()))
                    .collect();

                let code_names: Vec<(String, String)> = industry_rows
                    .iter()
                    .map(|(code, name, _)| (code.clone(), name.clone()))
                    .collect();

                let (bars, stats) = load_or_fetch_bars(state, repo, &code_names, date).await?;
                let boards = aggregate_boards(&bars, &industry_of, &member_counts);
                (boards, bars, None, "成分股日线等权算术平均".to_string(), stats)
            }
        };

    let (up, down, flat, covered, bar_amount) = calc_breadth(&bars);
    let total_amount = fast_amount.or(bar_amount);

    let indices = em.fetch_index_daily(date).await.unwrap_or_default();

    if total_amount.is_none() {
        notes.push(
            "成交额不可得：腾讯与新浪日线接口均不含成交额字段；\
             仅最近一个交易日可通过板块快照与指数成交额补齐，故此处不做估算。"
                .to_string(),
        );
    }
    if bars.is_empty() {
        // 区分两种「空」：日期本身不是交易日（探测层已拦掉），或上游限流返回空响应。
        // 后者在连续多次全量抓取后常见，提示用户稍后重试而不是当成真实市场状态。
        notes.push(
            "未能取到当日成分股日线：可能是行情源限流（冷启动需抓取全市场约 5600 只个股）。\
             请在几分钟后重试，本次结果不会写入缓存。"
                .to_string(),
        );
    } else if fetch_stats.failed > 0 {
        // 只有「真失败」才值得告警。停牌与未上市是市场本就没有当日数据，
        // 混进分母会让每一份日报都挂一条「数据可能不完整」的假警报。
        notes.push(format!(
            "成分股日线有 {} 只抓取失败（可能被行情源限流），市场宽度与行业涨跌幅\
             基于已取到的 {} 只计算，可能略低于真实覆盖面。",
            fetch_stats.failed,
            bars.len()
        ));
    }
    if fetch_stats.suspended > 0 || fetch_stats.not_traded > 0 {
        notes.push(format!(
            "另有 {} 只当日停牌、{} 只无行情数据（未上市或已退市），\
             属市场本身无当日数据，不计入市场宽度与行业涨跌幅。",
            fetch_stats.suspended, fetch_stats.not_traded
        ));
    }
    if lhb.record_count > lhb.stock_count {
        notes.push(format!(
            "龙虎榜 {} 只个股触发了多个上榜原因，共 {} 条记录；合计净买入按个股去重统计（取每只最大净买入额），明细保留全部记录。",
            lhb.record_count - lhb.stock_count,
            lhb.record_count
        ));
    }

    let elapsed_ms = started.elapsed().as_millis() as u64;

    Ok(SentimentDaily {
        date: date.to_string(),
        generated_at: chrono::Utc::now().timestamp(),
        lhb,
        themes,
        boards,
        breadth: appcore::models::MarketBreadth {
            up_count: up,
            down_count: down,
            flat_count: flat,
            covered_count: covered,
            total_amount,
            indices,
        },
        methodology: appcore::models::Methodology {
            board_method,
            from_cache: false,
            elapsed_ms,
            notes,
        },
        sources,
    })
}

/// 快路径：用东财板块快照 + 全市场快表直接构建行业榜与市场宽度
///
/// 只在「请求日期 == 快表最新交易日」时成立。板块快照是实时快照，盘中查当天会拿到
/// 尚未收盘的数据，而快表只在收盘后落库，用它当守卫天然等价于「该日行情已定稿」。
///
/// **任一步失败都整体返回 None**，不做部分采用——否则会出现「行业榜用官方口径、
/// 宽度用聚合口径」的混血报告，两条路径的覆盖家数还不一样，比纯慢路径更难解释。
async fn try_fast_path(
    em: &EastmoneyFetcher,
    member_counts: &HashMap<String, u32>,
    industry_universe: Option<&HashSet<String>>,
    date: &str,
) -> Option<(Vec<appcore::models::IndustryBoard>, Vec<DailyBar>, Option<f64>)> {
    if em.fetch_latest_trade_date().await.ok().flatten().as_deref() != Some(date) {
        return None;
    }

    let snaps = match fetch_with_retry("板块快照", FAST_RETRY_DELAY, || {
        em.fetch_board_snapshots()
    })
    .await
    {
        Some(s) if !s.is_empty() => s,
        // 空结果不重试：接口通了只是没数据，再问一次也是空的
        Some(_) => {
            tracing::warn!("板块快照返回空，回落到逐股聚合");
            return None;
        }
        None => return None,
    };

    let boards = boards_from_snapshots(&snaps, member_counts, industry_universe);
    if boards.is_empty() {
        tracing::warn!("板块快照与行业清单无交集，回落到逐股聚合");
        return None;
    }

    let market = match fetch_with_retry("全市场快表", FAST_RETRY_DELAY, || {
        em.fetch_market_snapshot()
    })
    .await
    {
        Some(m) if !m.is_empty() => m,
        _ => {
            tracing::warn!("全市场快表返回空或抓取失败，回落到逐股聚合");
            return None;
        }
    };

    let bars: Vec<DailyBar> = market
        .into_iter()
        .map(|m| DailyBar {
            code: m.code,
            name: m.name,
            close: m.close,
            pct_change: m.pct_change,
            amount: None,
        })
        .collect();

    // 快表没有成交额，用两市指数成交额单独补
    let total_amount = em.fetch_market_amount().await;
    Some((boards, bars, total_amount))
}

/// 带一次重试的抓取
///
/// 快路径的板块快照与全市场快表会偶发 `error decoding response body`（实测
/// 2026-09-02 12:44 命中一次，20:49 重试即正常，更像连续翻页触发的连接重置）。
/// 一次失败会让整条快路径整体放弃、回落到 110 秒的逐股聚合，并触发数百只限流。
///
/// 用户手点时这只是「慢一次」，但无人值守的定时任务一旦撞上，就会变成「每晚
/// 110 秒 + 限流 + 一份残缺日报被判定完整而落库」。所以这里重试一次，把瞬时
/// 故障与真正的接口故障区分开；重试仍失败就照旧回落，不把故障放大成几十秒的
/// 额外等待。
async fn fetch_with_retry<T, F, Fut>(label: &str, delay: Duration, mut f: F) -> Option<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = appcore::AppResult<T>>,
{
    match f().await {
        Ok(v) => Some(v),
        Err(first) => {
            tracing::warn!(step = label, error = %first, "快路径抓取失败，重试一次");
            tokio::time::sleep(delay).await;
            match f().await {
                Ok(v) => {
                    tracing::info!(step = label, "重试成功");
                    Some(v)
                }
                Err(second) => {
                    tracing::warn!(step = label, error = %second, "重试仍失败，回落到逐股聚合");
                    None
                }
            }
        }
    }
}

/// 日线抓取成败统计
///
/// 冷启动要发数千次请求，行情源（腾讯 `web.ifzq.gtimg.cn`）在达到阈值后会用
/// WAF 直接拦截（返回 501 + `waf.tencent.com` 页面），此时请求会「成功返回但
/// 内容为空」，解析层表现为解析失败。若不统计成功率，用户只会看到一份静默
/// 残缺的日报，无法区分「市场真的没数据」和「被拦了」。
#[derive(Debug, Clone, Copy, Default)]
struct BarFetchStats {
    attempted: usize,
    succeeded: usize,
    /// 行情源有该股 K 线，但不含目标日期 → 当日停牌
    suspended: usize,
    /// 行情源没有该标的任何 K 线 → 未上市 / 已退市
    not_traded: usize,
    /// 请求失败（网络错误 / 限流 / WAF），这才是真正需要告警的
    failed: usize,
}

/// 单只成分股的日线抓取结果
///
/// 「没拿到目标日期的 bar」有四种含义，只有最后一种才算数据缺失。
/// 分类信号全部来自抓取过程本身，不额外发请求：
/// - 行情源返回了 K 线但不含目标日期 → 停牌，或当天还没上市（见
///   [`outcome_for_missing_date`]，按最早一根 K 线的日期区分）
/// - 新浪对不认识的 symbol 返回字面量 `null` → 未上市 / 已退市
enum BarOutcome {
    Got(DailyBar),
    Suspended,
    NotTraded,
    Failed,
}

/// 单只成分股一次抓取的完整结果
///
/// 单独成结构而不是只返回 [`BarOutcome`]，是因为「目标日的结果」和「抓回来的序列」
/// 是两件事：前者决定这只股票怎么计入统计，后者决定哪些日期之后能免抓。停牌 /
/// 未上市的序列同样要落库——那天没成交，其余交易日照样有效。
struct FetchedStock {
    code: String,
    outcome: BarOutcome,
    /// 抓回来的整条序列。请求失败时为空
    klines: Vec<Kline>,
}

/// 有 K 线、但目标日期那根不在其中时，区分「当日停牌」与「当天还没上市」
///
/// 只看「有没有目标日期那根」会把两者混为一谈：最早一根 K 线都晚于目标日，
/// 说明那天该股尚未上市，跟停牌是完全不同的两回事。2026-07-24 有 23 只次新股
/// （嘉立创、宇树科技、双英集团…）因此被误报成「当日停牌」。
fn outcome_for_missing_date(klines: &[Kline], date: &str) -> BarOutcome {
    // 取最早一根而非 `first()`：不假设上游返回的序列一定按日期升序
    let earliest = klines.iter().map(|k| k.date.as_str()).min();
    match earliest {
        Some(d) if d > date => BarOutcome::NotTraded,
        _ => BarOutcome::Suspended,
    }
}

/// 由已缓存序列判出的「目标日无数据」原因
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeriesResolution {
    /// 目标日在序列区间内却没有那一根：当日停牌
    Suspended,
    /// 目标日早于序列起点，而序列已回溯到上市首日：那天还没上市
    NotTraded,
}

/// 某只股票的目标日能否**不请求行情源**就判定为「当天没数据」
///
/// 这是序列缓存最容易出错的一处判定，故抽成纯函数以便单测。
/// 缓存里没有某只股票的目标日，远不等于要去抓它——若它的序列今天刚抓过、
/// 且目标日落在已抓区间内，那「没有」只能说明那天它没交易。
///
/// - `Some(_)`：可以直接判定，见 [`SeriesResolution`]
/// - `None`：必须去抓
///
/// ## `fetched_at` 只比对日期，不看时间
///
/// 这是刻意的设计：腾讯是**前复权**，个股除权后它历史上的所有价格都会被重算。
/// 若序列长期缓存不刷，除权日一过缓存里的历史价就全是错的，而且**看不出来**。
/// 每天重抓一次，把陈旧性压到一天以内——归档任务本来就每天跑，等于顺路复用。
fn resolve_from_series(
    meta: Option<&storage::SeriesMeta>,
    today: &str,
    date: &str,
) -> Option<SeriesResolution> {
    let m = meta?;
    if m.fetched_at != today {
        return None;
    }

    if date < m.first_date.as_str() {
        // 早于序列起点。只有序列已回溯到上市首日（`complete`）才能断定那天尚未
        // 上市；否则是请求窗口不够长，得加大窗口重抓
        return m.complete.then_some(SeriesResolution::NotTraded);
    }

    // 落在序列区间内却没有这一根。目标日晚于 `last_date` 时也走这里（比如请求了
    // 一个还没到的日期），与 `outcome_for_missing_date` 的既有口径一致
    Some(SeriesResolution::Suspended)
}

/// 读取日线缓存，只补齐缺失的个股
///
/// 不做「有缓存就全用、否则全量重抓」的二选一：成分股清单会随新股上市、行业
/// 归属调整而变化，二选一会让缓存永远停在首次抓取的那一批。这里按代码比对，
/// 只请求缓存里没有的那部分，全量冷启动与日常增量走同一条路径。
///
/// 另一个好处是自愈：若上一次抓取因限流只拿到部分数据（空结果不落库，但部分
/// 成功会落库），下次请求会自动补齐剩余部分。
///
/// ## 缓存的是整条序列，不是「目标日那一行」
///
/// 行情源只支持「最近 N 根」，所以一次抓取能顺带覆盖 [`SERIES_WINDOW_BARS`]
/// 个交易日。把整条序列落库后，窗口内的其它日期再被查询时**一次请求都不用发**。
///
/// 有一个必须处理的坑：**缓存里没有某只股票的目标日，不等于要去抓它**。
/// 若它的序列今天已经抓过、且目标日落在该序列的区间内，那么「缓存里没有」只能
/// 说明那天它没交易——不该为此重抓，否则序列缓存等于白做。反之，若目标日早于
/// 序列起点、且序列没被窗口截断（`complete`），那就是那天还没上市。
async fn load_or_fetch_bars(
    state: &AppState,
    repo: &storage::Repository,
    code_names: &[(String, String)],
    date: &str,
) -> appcore::AppResult<(Vec<DailyBar>, BarFetchStats)> {
    let name_of: HashMap<String, String> = code_names.iter().cloned().collect();

    let cached = repo.get_daily_bars(date).await.unwrap_or_default();
    let cached_codes: HashSet<&str> = cached
        .iter()
        .map(|(code, _, _, _)| code.as_str())
        .collect();

    let mut bars: Vec<DailyBar> = cached
        .iter()
        .map(|(code, close, pct, amount)| DailyBar {
            name: name_of.get(code).cloned().unwrap_or_default(),
            code: code.clone(),
            close: *close,
            pct_change: *pct,
            // 腾讯日线不含成交额，落库时记为 0，此处还原为「缺失」
            amount: if *amount > 0.0 { Some(*amount) } else { None },
        })
        .collect();

    let missing: Vec<(String, String)> = code_names
        .iter()
        .filter(|(code, _)| !cached_codes.contains(code.as_str()))
        .cloned()
        .collect();

    if missing.is_empty() {
        return Ok((bars, BarFetchStats::default()));
    }

    // 本地日期。序列当天抓过就不再重抓，见上面的说明
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let metas = repo.all_series_meta().await.unwrap_or_default();

    let missing_len = missing.len();
    let mut need_fetch: Vec<(String, String)> = Vec::with_capacity(missing_len);
    let mut stats = BarFetchStats::default();
    for (code, name) in missing {
        match resolve_from_series(metas.get(&code), &today, date) {
            Some(SeriesResolution::Suspended) => stats.suspended += 1,
            Some(SeriesResolution::NotTraded) => stats.not_traded += 1,
            // 其余都要抓：从没抓过 / 昨天抓的（前复权可能已变）/ 窗口不够长
            None => need_fetch.push((code, name)),
        }
    }

    let hit_series = missing_len - need_fetch.len();
    if need_fetch.is_empty() {
        tracing::info!(
            date = %date,
            hit_series,
            "目标日全部落在已缓存的序列区间内，本次未请求行情源"
        );
        return Ok((bars, stats));
    }

    tracing::info!(
        date = %date,
        cached = cached_codes.len(),
        missing = missing_len,
        hit_series,
        to_fetch = need_fetch.len(),
        "补齐缺失的成分股日线"
    );

    // 固定窗口与「回溯到目标日所需根数」取大者：半年窗口覆盖日常回溯，
    // 更早的历史日期由 `bars_needed_for` 兜住
    let limit = SERIES_WINDOW_BARS.max(EastmoneyFetcher::bars_needed_for(date));
    // 取引用后再进 async move：引用可 Copy，避免整张表被首个异步块移走
    let name_of = &name_of;
    let fetcher = state.fetcher.clone();
    stats.attempted = need_fetch.len();

    let fetched: Vec<FetchedStock> = stream::iter(need_fetch.into_iter())
        .map(|(code, _)| {
            let fetcher = fetcher.clone();
            async move {
                // end 传目标日期：抓取层据此判定主源是否真覆盖了这一天，
                // 覆盖不到就自动回落到新浪兜底（详见 `fetch_kline_with_status` 注释）。
                // 不传的话主源只要返回非空 K 线就算成功，会把「主源历史窗口不够」
                // 误判成「当日停牌」——2026-07-24 曾因此虚报 328 只停牌。
                let (klines, status) = fetcher
                    .fetch_kline_with_status(&code, &KlinePeriod::Daily, None, Some(date), Some(limit))
                    .await
                    .unwrap_or_else(|_| (Vec::new(), KlineStatus::Error));

                let outcome = match status {
                    // 行情源根本没有这个标的：未上市 / 已退市 / 代码不存在
                    KlineStatus::NoData => BarOutcome::NotTraded,
                    // 两个源都没拿到且不是「无此标的」：请求层面失败
                    KlineStatus::Error => BarOutcome::Failed,
                    KlineStatus::Data => match klines.iter().position(|k| k.date == date) {
                        Some(i) => match klines[i].pct_change {
                            Some(pct) => BarOutcome::Got(DailyBar {
                                name: name_of.get(&code).cloned().unwrap_or_default(),
                                code: code.clone(),
                                close: klines[i].close,
                                pct_change: pct,
                                amount: None,
                            }),
                            // 拿到了 K 线却没有涨跌幅，属上游数据质量问题
                            None => BarOutcome::Failed,
                        },
                        None => outcome_for_missing_date(&klines, date),
                    },
                };

                // 序列原样带回来落库。注意「停牌 / 未上市」也是有效序列——那天没
                // 交易不代表其余交易日没有数据，落库后查别的日期同样能命中
                FetchedStock {
                    code,
                    outcome,
                    klines,
                }
            }
        })
        .buffer_unordered(BAR_CONCURRENCY)
        .collect()
        .await;

    // 整条序列落库，而不只是目标日那一行：这是「换日期不用重抓」的全部来源。
    // 停牌 / 未上市的序列同样落库——它们只是目标日没成交，其他交易日依然有效。
    store_series(
        repo,
        fetched.iter().map(|f| SeriesToStore {
            code: f.code.as_str(),
            klines: f.klines.as_slice(),
            limit,
            // 这条路传了 end，源必须覆盖到目标日才算成功，判定有依据
            allow_complete: true,
            // 库里若已有记录（例如只是 fetched_at 不是今天），沿用它的起点，
            // 别让一次小窗口补抓取把声明的覆盖往回收缩
            existing_first: metas.get(&f.code).map(|m| m.first_date.as_str()),
        }),
        &today,
    )
    .await;

    let mut got = 0usize;
    for f in fetched {
        match f.outcome {
            BarOutcome::Got(b) => {
                bars.push(b);
                got += 1;
            }
            BarOutcome::Suspended => stats.suspended += 1,
            BarOutcome::NotTraded => stats.not_traded += 1,
            BarOutcome::Failed => stats.failed += 1,
        }
    }
    stats.succeeded = got;

    // 告警只看「真失败」：停牌与未上市是市场本就没有数据，用成功率算会误报。
    // 增量补齐时 attempted 可能只有几十只，分母小，阈值取 10% 并同时看绝对值。
    if stats.attempted > 0 && stats.failed > 0 {
        let fail_rate = stats.failed as f64 / stats.attempted as f64;
        if fail_rate > 0.1 || stats.failed > 50 {
            // 两种成因的处置完全相反，必须分开说：主源熔断是**已知且已被兜底吸收**
            // 的情况，不需要干预；剩下的才是要降并发、错峰重试的真限流。
            // 混成一句「疑似限流或 WAF 拦截」，等于把已经处理掉的事和需要动手的
            // 事报成同一条，看日志的人无法据此行动。
            //
            // 熔断状态放在整批抓完之后读：批内前若干只还在试探主源，读到的是批末
            // 状态，恰好对应「这批数据实际走的是哪条路」。
            if state.fetcher.primary_source_tripped() {
                tracing::warn!(
                    date = %date,
                    failed = stats.failed,
                    attempted = stats.attempted,
                    suspended = stats.suspended,
                    not_traded = stats.not_traded,
                    "主源腾讯已熔断，本批日线全部由兜底源新浪提供；仍有失败需查兜底源是否限流"
                );
            } else {
                tracing::warn!(
                    date = %date,
                    failed = stats.failed,
                    attempted = stats.attempted,
                    suspended = stats.suspended,
                    not_traded = stats.not_traded,
                    "日线抓取失败率过高；主源状态正常，疑似兜底源限流或网络抖动，可降并发错峰重试"
                );
            }
        }
    }

    Ok((bars, stats))
}

/// 一条待落库的序列
struct SeriesToStore<'a> {
    code: &'a str,
    klines: &'a [Kline],
    /// 这只股票**实际使用**的请求根数。必须逐只带进来：刷新路径与日报路径的窗口
    /// 算法不同，统一按固定窗口判 `complete` 会标错。
    limit: u32,
    /// 库里**已经存在**的最早日期；落库时 `first_date` 取它与本次抓到的首日中
    /// 更早的那个。
    ///
    /// 为什么需要它：落库是 `ON CONFLICT DO UPDATE`，只增不删，所以库里的行永远
    /// 比「本次抓回来的那一批」多或相等。若 `first_date` 只按本次结果算，刷新时
    /// 窗口比历史积累的短，声明的覆盖就会往回收缩，查更早的日期会被无谓地判定
    /// 为「需要重抓」。反过来，若为了不收缩而把请求窗口开得比现状更大，窗口又会
    /// 每轮变大（估算函数必然高估），序列无界增长——两个方向都错。
    /// 保住覆盖的正解是：**请求窗口固定，声明的起点取历史与本次的最小值**。
    existing_first: Option<&'a str>,
    /// 是否允许据此断言「已回溯到上市首日」
    ///
    /// 滚动刷新必须传 `false`。它不传目标日，拿不到「源是否覆盖到某天」的证据，
    /// 而腾讯对部分标的返回窗口**短于请求根数**（2026-07-24 需 43 根时，北交所
    /// 920 段 343 只 100% 覆盖不到）。若照 `返回根数 < 请求根数` 断言上市首日，
    /// 等于把「腾讯窗口不够」说成「那天还没上市」——上层据此判「未交易」，报告
    /// 会静默少算一批股票，且不报任何错。
    ///
    /// 保守取 `false` 是安全的：代价只是查更早日期时多抓一次，抓完自然有正确
    /// 答案；反过来标错则直接产出错误结论。两种错误的代价完全不对称。
    allow_complete: bool,
}

/// 算一条序列的抓取记录；序列里没有一根可用 K 线则不产生记录
///
/// 抽成纯函数是为了能直接单测 `complete` 的判定——那是个只在高并发抓取后才
/// 写进库、出错时又完全静默的字段，没有单测等于没有验证。
fn build_series_meta(
    s: &SeriesToStore<'_>,
    today: &str,
) -> Option<storage::SeriesMetaRow> {
    let mut first: Option<&str> = None;
    let mut last: Option<&str> = None;

    for k in s.klines {
        // 首根 K 线没有涨跌幅：两家行情源都只给收盘价，涨跌幅要靠前一根现算
        // （见 `parse_sina_kline` 的注释）。落库就得写成 0，将来查到那天会是
        // 一个看起来正常、其实是假的数字——宁可这一根不存
        if k.pct_change.is_none() {
            continue;
        }
        if first.is_none_or(|d| k.date.as_str() < d) {
            first = Some(k.date.as_str());
        }
        if last.is_none_or(|d| k.date.as_str() > d) {
            last = Some(k.date.as_str());
        }
    }

    let (first, last) = (first?, last?);

    // 声明的起点 = min(本次抓到的首日, 库里已有的首日)。
    // 只取 min 不取本次值，是为了让「刷新」这件事对覆盖度**单调不可逆**：行情源
    // 这次返回得少（例如主源对某些标的的历史窗口偏短），也只是少刷新了几天，
    // 不会让已经落库的历史凭空从声明里消失。
    let first = match s.existing_first {
        Some(old) if old < first => old,
        _ => first,
    };

    Some(storage::SeriesMetaRow {
        stock_code: s.code.to_string(),
        fetched_at: today.to_string(),
        first_date: first.to_string(),
        last_date: last.to_string(),
        // 返回根数少于请求根数 = 上游没有更早的数据了，即已回溯到上市首日。
        // 但这只在「上游会把家底全给出来」时成立，所以受 `allow_complete` 约束
        complete: s.allow_complete && s.klines.len() < s.limit as usize,
    })
}

/// 把抓回来的整条序列落库，并更新「这只股票的序列是哪天抓的」
///
/// 抽成函数是因为有两条调用路径：**补齐日报**时的抓取，以及归档任务里的
/// **滚动刷新**（[`refresh_stale_series`]）。后者没有「目标日」这个概念，
/// 只关心序列本身，所以这里刻意不碰 `BarOutcome`。
async fn store_series<'a>(
    repo: &storage::Repository,
    series: impl IntoIterator<Item = SeriesToStore<'a>>,
    today: &str,
) {
    let mut rows: Vec<(String, String, f64, f64, f64)> = Vec::new();
    let mut meta_rows: Vec<storage::SeriesMetaRow> = Vec::new();

    for s in series {
        for k in s.klines {
            let Some(pct) = k.pct_change else { continue };
            rows.push((s.code.to_string(), k.date.clone(), k.close, pct, k.amount));
        }
        if let Some(m) = build_series_meta(&s, today) {
            meta_rows.push(m);
        }
    }

    // 序列缓存让单次写入从几千行涨到几十万行，写库本身成了冷启动的主要成本之一。
    // 记下行数与耗时，否则「冷启动慢了」会先被怀疑成抓取出问题
    if !rows.is_empty() {
        let t = std::time::Instant::now();
        let n = rows.len();
        let r = repo.save_daily_bars(&rows).await;
        tracing::info!(
            rows = n,
            elapsed_ms = t.elapsed().as_millis() as u64,
            ok = r.is_ok(),
            "日线序列落库"
        );
        if let Err(e) = r {
            tracing::warn!(error = %e, "日线缓存写入失败");
        }
    }
    if let Err(e) = repo.save_series_meta(&meta_rows).await {
        tracing::warn!(error = %e, rows = meta_rows.len(), "日线序列抓取记录写入失败");
    }
}

/// 滚动刷新的结果
pub(crate) struct SeriesRefreshResult {
    /// 成功刷新的只数
    pub refreshed: usize,
    /// 抓取失败的只数。它们的 `fetched_at` 没被更新，下一轮会再排到队首重试
    pub failed: usize,
    /// 重写了多少行日线
    pub rows: usize,
}

/// 重新抓取一批股票的日线序列，整条覆盖落库
///
/// ## 为什么需要它
///
/// 序列缓存有个副作用：**缓存一旦写满，就再也不会刷新**。某日期的缓存若已齐全，
/// [`load_or_fetch_bars`] 会在 `missing` 为空时直接返回，一只股票都不重抓。于是
/// 历史行就停在被写入那天的状态。
///
/// 新浪是**不复权**，价格不会变，停着也没事；但腾讯是**前复权**，个股除权后它
/// 历史上的所有价格都会被重算——缓存里的历史价就悄悄变成了错的，而且看不出来。
/// 所以只要数据来自前复权口径，就必须周期性重抓。这道判断在 jobs.rs 里做。
///
/// ## 为什么是「分片」而不是每天全量
///
/// 每天全量刷新 5,600+ 只，等于每天跑一次冷启动，序列缓存的意义就没了。所以
/// 刷新周期是个**预算旋钮**：每天刷 `总数 / 周期` 只，轮一遍即刷新一次全市场。
/// 一只股票刷新回来的序列（[`refresh_stale_series`] 的中间产物）
struct RefreshedSeries {
    code: String,
    klines: Vec<Kline>,
    limit: u32,
    allow_complete: bool,
    /// 库里已有的序列起点，落库时用来让声明的覆盖只增不减
    existing_first: String,
}

pub(crate) async fn refresh_stale_series(
    state: &AppState,
    repo: &storage::Repository,
    targets: &[(String, String)],
) -> SeriesRefreshResult {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let fetcher = state.fetcher.clone();

    // 现有记录里 `complete` 的取值。刷新只刷新价格，不刷新这个断言——它只能
    // 沿用旧值或置 false，绝不能把 false 升成 true（理由见 SeriesToStore）
    let known = repo.all_series_meta().await.unwrap_or_default();

    let got: Vec<RefreshedSeries> = stream::iter(targets.iter().cloned())
        .map(|(code, first_date)| {
            let fetcher = fetcher.clone();
            let allow_complete = known.get(&code).is_some_and(|m| m.complete);
            async move {
                // 固定窗口，不随现有覆盖放大。
                //
                // 早先这里取 `max(120, bars_needed_for(first_date))`，本意是
                // 「刷新不能缩短已有覆盖」。但 `bars_needed_for` 是「自然日 ×
                // 5/7 + 15」的估算，用在「已有序列的起点」上必然高估，于是每
                // 刷新一轮窗口就往前推一截：实测一轮多出 21,574 行（805 只，
                // 约 27 行/只），7 天一轮、一年 52 轮即无界增长。
                //
                // 保住覆盖不该靠放大窗口，而该靠落库时 `first_date` 取 min（见
                // `SeriesToStore::existing_first`）：老行不会被删，覆盖既不
                // 蠕变也不缩水。
                let limit = SERIES_WINDOW_BARS;

                // 不传 end：固定 120 根对绝大多数股票覆盖不到现有起点——5,215
                // 只的序列有 181~240 行——传了就每只都判定「源没覆盖到」而白白
                // 回落兜底源，请求翻倍且永远拿不到主源。覆盖不再由 end 保证，
                // 改由 `existing_first` 在落库侧保证。
                let (klines, status) = fetcher
                    .fetch_kline_with_status(&code, &KlinePeriod::Daily, None, None, Some(limit))
                    .await
                    .unwrap_or_else(|_| (Vec::new(), KlineStatus::Error));

                match status {
                    KlineStatus::Data if !klines.is_empty() => Some(RefreshedSeries {
                        code,
                        klines,
                        limit,
                        allow_complete,
                        existing_first: first_date,
                    }),
                    _ => None,
                }
            }
        })
        .buffer_unordered(BAR_CONCURRENCY)
        .filter_map(|o| async move { o })
        .collect()
        .await;

    let refreshed = got.len();
    let rows: usize = got.iter().map(|g| g.klines.len()).sum();

    // 批次内若发生主源翻转，这一批就会一半前复权、一半不复权——两套价格口径进
    // 同一份等权聚合，且不报任何错。这里先观测起来：翻转是能判定的（批次前后
    // 主源状态不一致），只是暂时还做不到「翻转就整批作废」（那需要抓取层把
    // 「这一只实际用了哪个源」报上来，见 docs/kline-source-convention-plan.html
    // 的方案 A1）。先把这件事变成日志里看得见的事件，而不是永远静默。
    let primary_up_at_start = !state.fetcher.primary_source_tripped();

    store_series(
        repo,
        got.iter().map(|g| SeriesToStore {
            code: g.code.as_str(),
            klines: g.klines.as_slice(),
            limit: g.limit,
            allow_complete: g.allow_complete,
            existing_first: Some(g.existing_first.as_str()),
        }),
        &today,
    )
    .await;

    if primary_up_at_start != !state.fetcher.primary_source_tripped() {
        tracing::warn!(
            batch = targets.len(),
            refreshed,
            "日线序列刷新批次内主源状态发生翻转：本批可能混入了前复权与不复权两套口径，\
             结果不可信，需人工核对"
        );
    }

    let failed = targets.len() - refreshed;
    if failed > 0 {
        tracing::warn!(
            batch = targets.len(),
            refreshed,
            failed,
            "日线序列滚动刷新有失败，这些股票的 fetched_at 未更新，下一轮会重试"
        );
    }
    SeriesRefreshResult {
        refreshed,
        failed,
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(fetched_at: &str, first_date: &str, complete: bool) -> storage::SeriesMeta {
        storage::SeriesMeta {
            fetched_at: fetched_at.to_string(),
            first_date: first_date.to_string(),
            last_date: "2026-09-03".to_string(),
            complete,
        }
    }

    /// 目标日落在已抓区间内却没有那一根 → 判定停牌，不必重抓
    #[test]
    fn test_resolve_from_series_hits_window() {
        let m = meta("2026-09-04", "2026-03-01", false);
        assert_eq!(
            resolve_from_series(Some(&m), "2026-09-04", "2026-06-30"),
            Some(SeriesResolution::Suspended)
        );
    }

    /// 目标日早于序列起点、且序列已回溯到上市首日 → 那天还没上市
    #[test]
    fn test_resolve_from_series_before_first_date_when_complete() {
        // 次新股：序列只有上市后的几根，complete=true
        let m = meta("2026-09-04", "2026-09-01", true);
        assert_eq!(
            resolve_from_series(Some(&m), "2026-09-04", "2026-06-30"),
            Some(SeriesResolution::NotTraded)
        );
    }

    /// 目标日早于序列起点、但序列是被窗口截断的 → 必须加大窗口重抓
    ///
    /// 这条是防「把窗口不够长误判成未上市」：老股票被 120 根窗口截断时
    /// `complete=false`，此时再早的日期不是没数据，是我们没要到
    #[test]
    fn test_resolve_from_series_before_first_date_when_truncated() {
        let m = meta("2026-09-04", "2026-03-01", false);
        assert_eq!(resolve_from_series(Some(&m), "2026-09-04", "2025-06-30"), None);
    }

    /// 昨天抓的序列一律重抓：腾讯是前复权，隔天不刷历史价可能已被重算
    #[test]
    fn test_resolve_from_series_stale_must_refetch() {
        let m = meta("2026-09-03", "2026-03-01", true);
        assert_eq!(resolve_from_series(Some(&m), "2026-09-04", "2026-06-30"), None);
    }

    /// 从没抓过 → 必须抓
    #[test]
    fn test_resolve_from_series_without_meta_must_fetch() {
        assert_eq!(resolve_from_series(None, "2026-09-04", "2026-06-30"), None);
    }

    /// 构造 `n` 根连续 K 线；首根无涨跌幅（两家行情源都如此，见 `store_series`）
    fn klines(n: usize) -> Vec<Kline> {
        (0..n)
            .map(|i| Kline {
                stock_code: "000001".into(),
                date: format!("2026-09-{:02}", i + 1),
                open: 1.0,
                close: 1.0 + i as f64,
                high: 1.0,
                low: 1.0,
                volume: 1.0,
                amount: 1.0,
                // 第 0 根没有前一根可比，源不给涨跌幅
                pct_change: if i == 0 { None } else { Some(0.5) },
                turnover: None,
            })
            .collect()
    }

    /// 序列只有首根（无涨跌幅）时不产生记录——否则会记下一个空区间
    #[test]
    fn test_build_series_meta_skips_all_head_series() {
        let k = klines(1);
        let s = SeriesToStore {
            code: "000001",
            klines: &k,
            limit: 120,
            allow_complete: true,
            existing_first: None,
        };
        assert!(
            build_series_meta(&s, "2026-09-04").is_none(),
            "没有一根可用 K 线就不该记抓取记录，否则 first/last 无从谈起"
        );
    }

    /// 返回根数少于请求根数 → 已回溯到上市首日
    #[test]
    fn test_build_series_meta_marks_complete_when_short() {
        let k = klines(11);
        let s = SeriesToStore {
            code: "000001",
            klines: &k,
            limit: 120,
            allow_complete: true,
            existing_first: None,
        };
        let m = build_series_meta(&s, "2026-09-04").unwrap();
        assert!(m.complete);
        // 首根被跳过（无涨跌幅），起点是第二根
        assert_eq!(m.first_date, "2026-09-02");
        assert_eq!(m.last_date, "2026-09-11");
        assert_eq!(m.fetched_at, "2026-09-04");
    }

    /// 返回根数等于请求根数 → 被窗口截断
    #[test]
    fn test_build_series_meta_not_complete_when_exhausted_limit() {
        let k = klines(120);
        let s = SeriesToStore {
            code: "000001",
            klines: &k,
            limit: 120,
            allow_complete: true,
            existing_first: None,
        };
        assert!(!build_series_meta(&s, "2026-09-04").unwrap().complete);
    }

    /// 滚动刷新**永远不能**把 complete 断言成 true
    ///
    /// 它不传目标日，拿不到「源是否覆盖到某天」的证据；腾讯对部分标的的返回
    /// 窗口又短于请求根数。若允许它断言，就会把「腾讯窗口不够」说成「那天还没
    /// 上市」，上层据此判「未交易」，报告静默少算一批股票。
    #[test]
    fn test_build_series_meta_refresh_never_claims_complete() {
        let k = klines(11);
        let s = SeriesToStore {
            code: "000001",
            klines: &k,
            limit: 120,
            allow_complete: false,
            existing_first: None,
        };
        assert!(
            !build_series_meta(&s, "2026-09-04").unwrap().complete,
            "刷新路径即使拿到「少于请求根数」也不能断言已到上市首日"
        );
    }

    /// 声明的覆盖**只能变长，不能变短**：库里已有的起点比本次抓到的更早时，
    /// 必须沿用库里那个。
    ///
    /// 这是「固定窗口刷新」能成立的前提。落库是 `ON CONFLICT DO UPDATE`，老行
    /// 不会被删；若 `first_date` 只按本次结果算，那么刷新窗口（固定 120 根）比
    /// 历史积累（多有 181~240 行）短时，声明的覆盖就会往回收缩，查更早的日期会
    /// 被无谓地判成「需要重抓」，而数据其实还在库里。
    /// 反向的错误同样要防：为不收缩就把窗口开得比现状大，会让窗口每轮变大
    /// （`bars_needed_for` 是估算，必然高估），序列无界增长。
    #[test]
    fn test_build_series_meta_keeps_existing_when_older() {
        let k = klines(11);
        let s = SeriesToStore {
            code: "000001",
            klines: &k,
            limit: 120,
            allow_complete: false,
            existing_first: Some("2026-08-01"),
        };
        assert_eq!(
            build_series_meta(&s, "2026-09-04").unwrap().first_date,
            "2026-08-01",
            "本次只抓到 09-02，但库里已有 08-01 的行，声明的起点必须是 08-01"
        );
    }

    /// 本次抓到的比库里更早时，声明的覆盖应当**随之延长**
    #[test]
    fn test_build_series_meta_extends_when_fetched_older() {
        let k = klines(11);
        let s = SeriesToStore {
            code: "000001",
            klines: &k,
            limit: 120,
            allow_complete: false,
            existing_first: Some("2026-09-05"),
        };
        assert_eq!(
            build_series_meta(&s, "2026-09-04").unwrap().first_date,
            "2026-09-02",
            "本次抓到了比库里更早的 09-02，声明的起点应取更早的那个"
        );
    }

    /// 联网实测滚动刷新的两条核心不变量
    ///
    /// 单测覆盖不到抓取本身，而这个函数的价值全在抓取后的落库判断上，所以必须
    /// 真跑一次。只刷 2 只，几秒完成；不进默认测试集（需要外网）。
    ///
    /// 断言的两条都是「错了不报错、只会让报告悄悄算错」的那类：
    /// ① 覆盖范围只能变长不能变短；② `complete` 只能沿用或置 false，不能升级。
    #[tokio::test]
    #[ignore = "需要外网访问"]
    async fn test_refresh_stale_series_live() {
        let db = storage::Database::in_memory().await.unwrap();
        let repo = storage::Repository::from_database(&db);
        let state = AppState::new(
            String::new(),
            Arc::new(fetcher::EastmoneyFetcher::new()),
            db,
            analyzer::AiService::new(
                analyzer::ai::LlmClient::from_env(),
                Arc::new(fetcher::EastmoneyFetcher::new()),
            ),
            std::path::PathBuf::from("data/reports"),
        );

        let old_first = "2026-09-01";
        for code in ["000001", "600519"] {
            repo.save_series_meta(&[storage::SeriesMetaRow {
                stock_code: code.to_string(),
                fetched_at: "2020-01-01".to_string(),
                first_date: old_first.to_string(),
                last_date: "2026-09-03".to_string(),
                complete: false,
            }])
            .await
            .unwrap();
        }

        let targets = repo.oldest_series(2).await.unwrap();
        assert_eq!(targets.len(), 2);
        let r = refresh_stale_series(&state, &repo, &targets).await;

        assert_eq!(r.refreshed, 2, "两只都应刷新成功");
        assert_eq!(r.failed, 0);
        assert!(r.rows > 100, "应落库一整条序列，实际 {} 行", r.rows);

        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let metas = repo.all_series_meta().await.unwrap();
        for code in ["000001", "600519"] {
            let m = metas.get(code).expect("刷新后应仍有记录");
            assert_eq!(m.fetched_at, today, "{code} 的抓取日期应更新到今天");
            assert!(
                m.first_date.as_str() <= old_first,
                "{code} 的序列起点 {} 不应比刷新前的 {old_first} 更晚——刷新只许延长覆盖",
                m.first_date
            );
            assert!(
                !m.complete,
                "{code} 刷新路径不得断言「已回溯到上市首日」：它拿不到覆盖证据"
            );
        }
    }

    #[test]
    fn test_normalize_date_iso() {
        assert_eq!(normalize_date("2026-07-24").as_deref(), Some("2026-07-24"));
    }

    #[test]
    fn test_normalize_date_compact() {
        assert_eq!(normalize_date("20260724").as_deref(), Some("2026-07-24"));
    }

    #[test]
    fn test_normalize_date_invalid() {
        assert_eq!(normalize_date("2026/07/24"), None);
        assert_eq!(normalize_date("24-07-2026"), None);
        assert_eq!(normalize_date(""), None);
        // 不存在的日期应被 chrono 拒绝
        assert_eq!(normalize_date("2026-02-30"), None);
    }

    fn sample_report(covered: u32, boards: usize) -> SentimentDaily {
        SentimentDaily {
            date: "2026-07-24".to_string(),
            generated_at: 0,
            lhb: summarize_lhb(Vec::new(), 8),
            themes: build_theme_summary(Vec::new()),
            boards: (0..boards)
                .map(|i| appcore::models::IndustryBoard {
                    name: format!("行业{i}"),
                    change_pct: 0.0,
                    up_count: 0,
                    down_count: 0,
                    flat_count: 0,
                    leader: None,
                    leader_code: None,
                    leader_pct: 0.0,
                    amount: None,
                    member_count: 0,
                    calc_count: 0,
                })
                .collect(),
            breadth: appcore::models::MarketBreadth {
                up_count: 0,
                down_count: 0,
                flat_count: 0,
                covered_count: covered,
                total_amount: None,
                indices: Vec::new(),
            },
            methodology: appcore::models::Methodology {
                board_method: String::new(),
                from_cache: false,
                elapsed_ms: 0,
                notes: Vec::new(),
            },
            sources: Vec::new(),
        }
    }

    #[test]
    fn test_is_degraded_on_empty_bars() {
        // 行情源被限流时全市场日线抓成空，此时绝不能写缓存
        assert!(is_degraded(&sample_report(0, 0)));
        assert!(is_degraded(&sample_report(0, 128)));
    }

    #[test]
    fn test_is_degraded_on_empty_boards() {
        assert!(is_degraded(&sample_report(5103, 0)));
    }

    #[test]
    fn test_not_degraded_when_data_present() {
        assert!(!is_degraded(&sample_report(5103, 128)));
    }

    fn kline_on(date: &str) -> Kline {
        Kline {
            stock_code: "000001".to_string(),
            date: date.to_string(),
            open: 1.0,
            close: 1.0,
            high: 1.0,
            low: 1.0,
            volume: 0.0,
            amount: 0.0,
            pct_change: Some(0.0),
            turnover: None,
        }
    }

    #[test]
    fn test_missing_date_after_first_bar_means_not_yet_listed() {
        // 嘉立创 2026-08-04 才上市，07-24 那天市场上还没有这只股 → 不是停牌。
        // 2026-07-24 的日报曾把 23 只这类次新股误报成「当日停牌」。
        let klines = vec![kline_on("2026-08-04"), kline_on("2026-08-05")];
        assert!(matches!(
            outcome_for_missing_date(&klines, "2026-07-24"),
            BarOutcome::NotTraded
        ));
    }

    #[test]
    fn test_missing_date_between_bars_means_suspended() {
        // 联创电子 07-22 后停牌：目标日前后都有 K 线，唯独当天没有
        let klines = vec![kline_on("2026-07-22"), kline_on("2026-07-28")];
        assert!(matches!(
            outcome_for_missing_date(&klines, "2026-07-24"),
            BarOutcome::Suspended
        ));

        // 瓦轴B 停到 02-27：末条早于目标日，同样是停牌而非未上市
        let klines = vec![kline_on("2026-02-26"), kline_on("2026-02-27")];
        assert!(matches!(
            outcome_for_missing_date(&klines, "2026-07-24"),
            BarOutcome::Suspended
        ));
    }

    #[test]
    fn test_missing_date_ignores_bar_order() {
        // 不假设上游一定按日期升序返回，逆序也要判对
        let klines = vec![kline_on("2026-08-20"), kline_on("2026-08-04")];
        assert!(matches!(
            outcome_for_missing_date(&klines, "2026-07-24"),
            BarOutcome::NotTraded
        ));
    }

    /// 按调用次数依次返回预设结果的假抓取函数
    fn flaky(results: Vec<appcore::AppResult<i32>>) -> impl FnMut() -> std::pin::Pin<Box<dyn Future<Output = appcore::AppResult<i32>>>>
    {
        let mut it = results.into_iter();
        move || {
            let next = it.next().expect("调用次数超出预设");
            Box::pin(async move { next })
        }
    }

    #[tokio::test]
    async fn test_fetch_with_retry_recovers_from_one_failure() {
        // 瞬时故障：第二次成功。这正是快路径偶发连接重置的情形
        let out = fetch_with_retry(
            "t",
            Duration::ZERO,
            flaky(vec![
                Err(appcore::AppError::Network("error decoding response body".into())),
                Ok(7),
            ]),
        )
        .await;
        assert_eq!(out, Some(7));
    }

    #[tokio::test]
    async fn test_fetch_with_retry_does_not_retry_success() {
        // 首次就成功时不该再发第二次请求
        let out = fetch_with_retry("t", Duration::ZERO, flaky(vec![Ok(1), Ok(2)])).await;
        assert_eq!(out, Some(1));
    }

    #[tokio::test]
    async fn test_fetch_with_retry_gives_up_after_one_retry() {
        // 只重试一次，不把故障放大成几十秒的额外等待
        let out = fetch_with_retry(
            "t",
            Duration::ZERO,
            flaky(vec![
                Err(appcore::AppError::Network("a".into())),
                Err(appcore::AppError::Network("b".into())),
                Ok(3),
            ]),
        )
        .await;
        assert_eq!(out, None, "重试一次仍失败就应放弃，不该吃到第三次的结果");
    }
}
