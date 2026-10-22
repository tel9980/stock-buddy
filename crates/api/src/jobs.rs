//! 定时任务体（由 api 注入调度引擎）
//!
//! `scheduler` 只管「何时跑」，这里定义「跑什么」。两者分开是为了让调度层
//! 不必感知业务依赖——日报生成需要 fetcher + db + 渲染，全在本 crate 里。

use std::sync::Arc;

use scheduler::{FnJob, JobFuture, JobRun};

use crate::routes::sentiment::{
    archive_export, build_daily, cache_if_complete, is_degraded, refresh_stale_series,
};
use crate::routes::sentiment_report;
use crate::state::AppState;

/// 情绪日报自动归档
pub const JOB_SENTIMENT_ARCHIVE: &str = "sentiment-archive";
/// 日线缓存滚动清理
pub const JOB_CACHE_PRUNE: &str = "daily-bar-cache-prune";
/// 日线序列滚动刷新
pub const JOB_SERIES_REFRESH: &str = "kline-series-refresh";

/// 日线缓存保留多少个交易日。
///
/// 自动归档后这张表每天稳定涨约 5,600 行（年增约 137 万行 / 160 MB）。留 250
/// 个交易日 ≈ 一年，够回溯又不至于让库无限膨胀。被清掉的日期仍能重新生成，
/// 只是要重抓一次全市场——已归档的日报本身在 `sentiment_daily` 里，不受影响。
const CACHE_KEEP_DATES: usize = 250;

/// 全市场的日线序列滚动刷新一遍需要几天
///
/// 这是纯粹的**预算旋钮**，不是技术限制。方案最初设想「每天最多刷新一次」，
/// 但真按每天全量刷新 5,600+ 只，等于每天跑一次冷启动（≈18s + 67 万行写入），
/// 序列缓存的意义就没了。
///
/// 取 7 天一轮 → 每天约 805 只：约 800 次请求（≈4s）+ 9.7 万行更新（≈0.3s），
/// 换来「历史行最多陈旧 7 天」的确定性上界。想收紧就调小，代价线性上升。
const SERIES_REFRESH_CYCLE_DAYS: usize = 7;

/// 日线序列滚动刷新任务（每天一次，不参与启动补跑）
pub fn series_refresh_job(
    state: Arc<AppState>,
) -> FnJob<impl Fn(String) -> JobFuture + Send + Sync> {
    FnJob::new(JOB_SERIES_REFRESH, move |date: String| {
        let state = state.clone();
        Box::pin(async move { run_series_refresh(state, date).await })
    })
    .skip_catchup()
}

/// 情绪日报自动归档任务
pub fn sentiment_archive_job(
    state: Arc<AppState>,
) -> FnJob<impl Fn(String) -> JobFuture + Send + Sync> {
    FnJob::new(JOB_SENTIMENT_ARCHIVE, move |date: String| {
        let state = state.clone();
        Box::pin(async move { run_sentiment_archive(state, date).await })
    })
}

/// 日线缓存清理任务（每天一次，不参与启动补跑）
pub fn cache_prune_job(state: Arc<AppState>) -> FnJob<impl Fn(String) -> JobFuture + Send + Sync> {
    FnJob::new(JOB_CACHE_PRUNE, move |date: String| {
        let state = state.clone();
        Box::pin(async move { run_cache_prune(state, date).await })
    })
    .skip_catchup()
}

/// 生成并归档一个交易日的情绪日报
async fn run_sentiment_archive(state: Arc<AppState>, date: String) -> JobRun {
    let repo = storage::Repository::from_database(&state.db);

    // ① 有人正在手动生成这一天 → 让路。
    //    两份各 5,600 次抓取撞在一起，结果必然是双双被限流。
    if state.date_in_flight(&date) {
        return JobRun::skipped("用户在页面上正在生成该日日报，定时任务让路");
    }

    // ② 幂等：已有归档就不再跑。补跑会连着问好几天，这道判断是它的刹车。
    match repo.get_sentiment_daily(&date).await {
        // detail 会直接显示在前端归档抽屉顶部，脱离上下文也要读得懂
        Ok(Some(_)) => return JobRun::skipped("该日已归档，无需重复生成"),
        Ok(None) => {}
        Err(e) => return JobRun::failed(format!("查询归档失败：{e}")),
    }

    let started = std::time::Instant::now();
    let _guard = state.begin_date(&date);

    let report = match build_daily(&state, &repo, &date).await {
        Ok(r) => r,
        Err(e) => {
            return JobRun::failed(format!("生成失败：{e}"));
        }
    };

    // ③ 残缺不落库。与手动请求共用同一个守卫：无人值守时若把限流产生的空报告
    //    固化进归档，用户第二天打开看到的会是一份「市场全空」的假象。
    if is_degraded(&report) {
        return JobRun::failed(format!(
            "数据不完整（覆盖 {} 只 / {} 个行业），本次不归档，下一轮自动重试",
            report.breadth.covered_count,
            report.boards.len()
        ));
    }

    cache_if_complete(&repo, &report).await;

    // ④ 顺手渲染落盘：只要 4 ms，却能让归档目录里那天的「下载 HTML」直接可用
    let html = sentiment_report::render(&report);
    let exported = archive_export(&state, &report.date, &html);

    JobRun::ok(format!(
        "已归档：覆盖 {} 只 / {} 个行业，耗时 {:.1}s{}",
        report.breadth.covered_count,
        report.boards.len(),
        started.elapsed().as_secs_f64(),
        if exported { "" } else { "，但导出件留存失败" }
    ))
}

/// 清理过期的日线缓存
async fn run_cache_prune(state: Arc<AppState>, date: String) -> JobRun {
    let repo = storage::Repository::from_database(&state.db);

    // 每天只该跑一次。探测式触发每 15 分钟一轮，没有这道闸就会一天扫几十遍。
    match repo.has_successful_run(JOB_CACHE_PRUNE, &date).await {
        Ok(true) => return JobRun::skipped("今日已清理"),
        Ok(false) => {}
        Err(e) => return JobRun::failed(format!("查询运行记录失败：{e}")),
    }

    match repo.prune_daily_bars(CACHE_KEEP_DATES).await {
        Ok(0) => JobRun::ok(format!("无需清理（不足 {CACHE_KEEP_DATES} 个交易日）")),
        Ok(n) => JobRun::ok(format!(
            "清理 {n} 行，保留最近 {CACHE_KEEP_DATES} 个交易日"
        )),
        Err(e) => JobRun::failed(format!("清理失败：{e}")),
    }
}

/// 滚动刷新一部分日线序列，压制前复权带来的陈旧性
///
/// 缓存一旦写满就不会再刷新：某日期的缓存若已齐全，`load_or_fetch_bars` 会在
/// `missing` 为空时直接返回，一只股票都不重抓（详见该函数的注释）。历史行因此
/// 停在被写入那天的状态——只有这个任务会去动它们。
async fn run_series_refresh(state: Arc<AppState>, date: String) -> JobRun {
    let repo = storage::Repository::from_database(&state.db);

    match repo.has_successful_run(JOB_SERIES_REFRESH, &date).await {
        Ok(true) => return JobRun::skipped("今日已刷新"),
        Ok(false) => {}
        Err(e) => return JobRun::failed(format!("查询运行记录失败：{e}")),
    }

    // 不复权的数据永远不会陈旧，刷新它不只是浪费，还会引入口径混用。
    //
    // 腾讯是**前复权**：个股除权后它历史上的价格都会被重算，缓存会悄悄变错，
    // 必须周期性重抓。新浪是**不复权**：价格写完就不变，刷新毫无意义；更糟的
    // 是此时刷新会让这批股票换成新浪口径，而其余股票仍是腾讯口径，行业等权
    // 聚合会把两套价格混着算——正是主源熔断（SourceBreaker）要避免的情况。
    // 熔断中表示整批日线都走了新浪，正好就是「无需刷新」的判据。
    if state.fetcher.primary_source_tripped() {
        return JobRun::skipped(
            "主源腾讯熔断中，日线全为不复权口径，价格不会随除权变化，本次不刷新",
        );
    }

    let total = match repo.count_series_meta().await {
        Ok(n) => n as usize,
        Err(e) => return JobRun::failed(format!("查询序列记录失败：{e}")),
    };
    if total == 0 {
        return JobRun::skipped("尚无日线序列记录，等首次抓取后再开始滚动刷新");
    }

    // 向上取整，保证最后一批也轮得到
    let batch = total.div_ceil(SERIES_REFRESH_CYCLE_DAYS);
    let targets = match repo.oldest_series(batch).await {
        Ok(t) => t,
        Err(e) => return JobRun::failed(format!("查询待刷新序列失败：{e}")),
    };

    let started = std::time::Instant::now();
    let r = refresh_stale_series(&state, &repo, &targets).await;

    // 失败的股票不会更新 fetched_at，下一轮仍排在队首，等于自动重试
    JobRun::ok(format!(
        "滚动刷新 {} 只（{}/{}，{} 天一轮），重写约 {} 行，失败 {} 只，耗时 {:.1}s",
        r.refreshed,
        batch,
        total,
        SERIES_REFRESH_CYCLE_DAYS,
        r.rows,
        r.failed,
        started.elapsed().as_secs_f64()
    ))
}
