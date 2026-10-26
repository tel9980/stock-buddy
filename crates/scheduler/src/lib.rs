//! 通用定时调度引擎
//!
//! 只负责「何时跑」，不关心「跑什么」——任务体由调用方（api）以闭包注入。
//! 这样调度层不必感知业务依赖：日报生成需要 fetcher + db + 渲染三样东西，
//! 它们全在 api 里；若把编排下沉到 analyzer，分析层就得变存储感知。
//! 未来做T 提醒、自选股告警、行业轮动快照都能挂到同一个 tick 上。
//!
//! ## 为什么是「探测式触发」而不是固定时刻
//!
//! 直觉做法是定一个 crontab 式的「每交易日 15:30 跑一次」。这里刻意不这么做，
//! 改成每轮先问三个问题，全满足才动手：
//!
//! 1. 今天是周末吗 → 是则直接跳过，连探测请求都不发（省掉约 30% 的无效请求）
//! 2. 最新交易日 == 今天吗 → 不等于说明当日行情尚未落库，下一轮再看
//! 3. 任务体自己判断幂等（查归档表）→ 已有则返回 Skipped
//!
//! 换来四个好处：不需要维护节假日日历（元旦/春节/国庆休市无规律）、服务停机
//! 期间不会错过（下次启动自然补上）、东财延迟落库不会拿到空数据、中途失败
//! 下一轮天然重试且带退避。代价是每天约几十次轻量探测，每次只取 1 行。

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use storage::Repository;

/// 任务体的异步返回类型
///
/// `Box<dyn Future>` 默认 `+ 'static`，任务体若要借用外部数据必须先克隆进
/// async 块（通常是克隆一个 `Arc<AppState>`）。
pub type JobFuture = Pin<Box<dyn Future<Output = JobRun> + Send>>;

/// 交易日探测器的异步返回类型
pub type ProbeFuture<'a> = Pin<Box<dyn Future<Output = Option<String>> + Send + 'a>>;

/// 一次任务运行的结论
///
/// 只有 `Succeeded` 与 `Failed` 会真正消耗一次调度；`Skipped` 表示「条件不
/// 满足，本轮不该跑」，与失败是两回事——混在一起会让失败告警被每天几十次的
/// 「已归档，跳过」淹没。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobOutcome {
    Succeeded,
    Skipped,
    Failed,
}

impl JobOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            JobOutcome::Succeeded => "succeeded",
            JobOutcome::Skipped => "skipped",
            JobOutcome::Failed => "failed",
        }
    }
}

/// 任务运行结果
#[derive(Debug, Clone)]
pub struct JobRun {
    pub outcome: JobOutcome,
    /// 一句话说明，进 `job_runs.detail` 与日志
    pub detail: String,
}

impl JobRun {
    pub fn ok(detail: impl Into<String>) -> Self {
        Self {
            outcome: JobOutcome::Succeeded,
            detail: detail.into(),
        }
    }
    pub fn skipped(detail: impl Into<String>) -> Self {
        Self {
            outcome: JobOutcome::Skipped,
            detail: detail.into(),
        }
    }
    pub fn failed(detail: impl Into<String>) -> Self {
        Self {
            outcome: JobOutcome::Failed,
            detail: detail.into(),
        }
    }
}

/// 可被调度的任务
pub trait Job: Send + Sync {
    fn name(&self) -> &str;

    /// 是否参与启动补跑
    ///
    /// 每日一次性的维护任务（如缓存清理）应返回 `false`——对历史日期跑没有意
    /// 义，补跑三天就会白跑三次。
    fn run_on_catchup(&self) -> bool {
        true
    }

    fn run(&self, date: &str) -> JobFuture;
}

/// 用闭包充当任务，供 api 注入任务体
pub struct FnJob<F> {
    name: String,
    run_on_catchup: bool,
    f: F,
}

impl<F> FnJob<F>
where
    F: Fn(String) -> JobFuture + Send + Sync,
{
    pub fn new(name: impl Into<String>, f: F) -> Self {
        Self {
            name: name.into(),
            run_on_catchup: true,
            f,
        }
    }

    /// 标记为不参与启动补跑（每日一次性的维护任务）
    pub fn skip_catchup(mut self) -> Self {
        self.run_on_catchup = false;
        self
    }
}

impl<F> Job for FnJob<F>
where
    F: Fn(String) -> JobFuture + Send + Sync,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn run_on_catchup(&self) -> bool {
        self.run_on_catchup
    }

    fn run(&self, date: &str) -> JobFuture {
        (self.f)(date.to_string())
    }
}

/// 最新交易日探测器
///
/// 抽象成 trait 而非直接调 fetcher，是为了让调度逻辑能脱离网络做单测。
pub trait TradeDateProbe: Send + Sync {
    fn latest_trade_date(&self) -> ProbeFuture<'_>;
}

/// 用东财全市场快表探测最新交易日
///
/// 该报表的 `TRADE_DATE` 只在收盘后落库，所以它同时是「当日行情已定稿」的信号。
pub struct EmTradeDateProbe {
    fetcher: fetcher::EastmoneyFetcher,
}

impl EmTradeDateProbe {
    pub fn new(fetcher: fetcher::EastmoneyFetcher) -> Self {
        Self { fetcher }
    }
}

impl TradeDateProbe for EmTradeDateProbe {
    fn latest_trade_date(&self) -> ProbeFuture<'_> {
        Box::pin(async {
            // 探测失败按「无数据」处理：下一轮再试，不把它当错误上报
            self.fetcher.fetch_latest_trade_date().await.ok().flatten()
        })
    }
}

#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// 两轮探测之间的间隔
    pub interval: Duration,
    /// 启动补跑最多回溯几个工作日；0 表示不补跑
    pub catchup_days: usize,
    /// 补跑相邻两天之间的间隔
    pub catchup_gap: Duration,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(15 * 60),
            catchup_days: 3,
            catchup_gap: Duration::from_secs(2),
        }
    }
}

pub struct Scheduler {
    config: SchedulerConfig,
    probe: Arc<dyn TradeDateProbe>,
    jobs: Vec<Arc<dyn Job>>,
    /// 正在运行中的 "任务名:日期"
    ///
    /// 任务可能耗时数十秒（历史交易日的日报要走全市场抓取），若不加保护，
    /// 下一轮 tick 会再起一个同样的任务，堆叠起来必然触发行情源限流。
    running: Mutex<HashSet<String>>,
    /// 运行记录落库用的连接；`None` 表示不记录（单测）
    db: Option<storage::Database>,
}

impl Scheduler {
    pub fn new(config: SchedulerConfig, probe: Arc<dyn TradeDateProbe>) -> Self {
        Self {
            config,
            probe,
            jobs: Vec::new(),
            running: Mutex::new(HashSet::new()),
            db: None,
        }
    }

    /// 开启运行记录（写入 `job_runs` 表，供前端展示上次自动归档结果）
    pub fn with_db(mut self, db: storage::Database) -> Self {
        self.db = Some(db);
        self
    }

    pub fn register<J: Job + 'static>(&mut self, job: J) {
        self.jobs.push(Arc::new(job));
    }

    /// 单轮探测，返回实际成功执行的任务数
    pub async fn tick(&self) -> usize {
        self.tick_on(&local_date()).await
    }

    /// 以指定日期作为「今天」跑一轮（供单测驱动）
    pub async fn tick_on(&self, today: &str) -> usize {
        if is_weekend(today) {
            tracing::debug!(date = %today, "非工作日，跳过本轮调度");
            return 0;
        }

        let latest = match self.probe.latest_trade_date().await {
            Some(d) => d,
            None => {
                tracing::debug!(date = %today, "未能探测到最新交易日，跳过本轮");
                return 0;
            }
        };

        if latest != today {
            tracing::debug!(
                today = %today,
                latest = %latest,
                "当日行情尚未落库，跳过本轮"
            );
            return 0;
        }

        let mut ran = 0;
        for job in &self.jobs {
            if self.run_job(job.as_ref(), today).await {
                ran += 1;
            }
        }
        ran
    }

    /// 启动补跑
    ///
    /// 服务停机期间不会有人打开页面，归档会留下一屏空洞。这里回溯最近几个
    /// 工作日逐个交给任务体——缺不缺由任务体自己判断（查归档表），引擎不感知
    /// 业务。必须串行 + 间隔：每个历史交易日都是一次全市场抓取，并发会把行情
    /// 源打挂，反而补不齐。
    pub async fn catch_up(&self) {
        if self.config.catchup_days == 0 {
            return;
        }
        // 从旧到新：先补齐历史，最后处理今天
        for date in recent_workdays(self.config.catchup_days) {
            for job in &self.jobs {
                if !job.run_on_catchup() {
                    continue;
                }
                self.run_job(job.as_ref(), &date).await;
            }
            tokio::time::sleep(self.config.catchup_gap).await;
        }
    }

    /// 启动后台调度循环
    ///
    /// 先补跑再进循环：补跑期间自然包含了今天，且串行执行可避免与 tick 抢行情源。
    /// 返回的 JoinHandle 可以直接丢弃——tokio 的 task 不会因为 handle 被 drop
    /// 而取消。
    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.catch_up().await;
            loop {
                // tick 内部若 panic 会让整个调度停摆，这是刻意的：静默吞掉
                // panic 会让故障彻底不可见，宁可停在这里被日志追到。
                self.tick().await;
                // 交易时段感知间隔：休市/午休时段拉长休眠，减少对数据源的
                // 无效轮询（设计参考 quant-desktop 的 MarketSession 分级思想）。
                // 历史补数类任务不受行情时段影响，取两者较小值保守处理。
                let session = fetcher::MarketSession::now();
                let market_backoff = Duration::from_secs(session.poll_interval(true));
                let wait = self.config.interval.min(market_backoff);
                tokio::time::sleep(wait).await;
            }
        })
    }

    /// 跑一个任务并记账，返回是否成功执行（Skipped 不算）
    async fn run_job(&self, job: &dyn Job, date: &str) -> bool {
        let key = format!("{}:{}", job.name(), date);
        {
            let mut running = self.running.lock().await;
            if !running.insert(key.clone()) {
                tracing::debug!(job = job.name(), date = %date, "上一轮尚未结束，跳过");
                return false;
            }
        }

        let started = std::time::Instant::now();
        let result = job.run(date).await;
        let elapsed_ms = started.elapsed().as_millis() as u64;

        self.running.lock().await.remove(&key);

        match result.outcome {
            JobOutcome::Succeeded => tracing::info!(
                job = job.name(), date = %date, elapsed_ms, "{}", result.detail
            ),
            JobOutcome::Skipped => tracing::debug!(
                job = job.name(), date = %date, "{}", result.detail
            ),
            JobOutcome::Failed => tracing::warn!(
                job = job.name(), date = %date, elapsed_ms, "{}", result.detail
            ),
        }

        let outcome = result.outcome;
        self.record(job.name(), date, result, elapsed_ms).await;

        matches!(outcome, JobOutcome::Succeeded)
    }

    /// 写运行记录。失败只记日志——可观测性不该反过来拖垮任务本身
    async fn record(&self, job: &str, date: &str, run: JobRun, elapsed_ms: u64) {
        let Some(db) = &self.db else { return };
        let repo = Repository::from_database(db);
        if let Err(e) = repo
            .record_job_run(job, date, run.outcome.as_str(), &run.detail, Some(elapsed_ms))
            .await
        {
            tracing::warn!(job, date = %date, error = %e, "任务运行记录写入失败");
        }
    }
}

fn local_date() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// 最近 N 个工作日（含今天），按日期升序返回
fn recent_workdays(n: usize) -> Vec<String> {
    let today = chrono::Local::now().date_naive();
    let mut out = Vec::new();
    let mut back: i64 = 0;
    // 上限 30 天：即便配置写错也不会退化成无限循环
    while out.len() < n && back < 30 {
        let d = (today - chrono::Duration::days(back))
            .format("%Y-%m-%d")
            .to_string();
        back += 1;
        if is_weekend(&d) {
            continue;
        }
        out.push(d);
    }
    out.reverse();
    out
}

/// 星期几，1 = 周一 … 7 = 周日
///
/// 用 `%u` 而非 `NaiveDate::weekday()`：后者在当前 chrono 版本中是私有方法
/// （与 api 侧 sentiment.rs 的 `weekday_num` 同源，两处保持一致）。
fn weekday_num(date: &str) -> Option<u32> {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()?
        .format("%u")
        .to_string()
        .parse::<u32>()
        .ok()
}

fn is_weekend(date: &str) -> bool {
    matches!(weekday_num(date), Some(d) if d >= 6)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeProbe {
        latest: Option<String>,
        calls: Mutex<usize>,
    }

    impl FakeProbe {
        fn new(latest: Option<&str>) -> Self {
            Self {
                latest: latest.map(|s| s.to_string()),
                calls: Mutex::new(0),
            }
        }
    }

    impl TradeDateProbe for FakeProbe {
        fn latest_trade_date(&self) -> ProbeFuture<'_> {
            Box::pin(async {
                *self.calls.lock().await += 1;
                self.latest.clone()
            })
        }
    }

    /// 记录自己被调用过的日期，并返回预设结果
    fn recording_job(
        name: &str,
        log: Arc<Mutex<Vec<String>>>,
        run: JobRun,
    ) -> FnJob<impl Fn(String) -> JobFuture + Send + Sync> {
        FnJob::new(name, move |date: String| {
            let log = log.clone();
            let run = run.clone();
            Box::pin(async move {
                log.lock().await.push(date);
                run
            })
        })
    }

    fn scheduler_with(probe: FakeProbe, job: impl Job + 'static) -> Scheduler {
        let mut s = Scheduler::new(SchedulerConfig::default(), Arc::new(probe));
        s.register(job);
        s
    }

    #[tokio::test]
    async fn test_tick_skips_on_weekend_without_probing() {
        // 2026-09-05 是周六。周末连探测请求都不该发——省掉约 30% 的无效请求
        let log = Arc::new(Mutex::new(Vec::new()));
        let s = scheduler_with(
            FakeProbe::new(Some("2026-09-05")),
            recording_job("j", log.clone(), JobRun::ok("")),
        );

        assert_eq!(s.tick_on("2026-09-05").await, 0);
        assert!(log.lock().await.is_empty());
    }

    #[tokio::test]
    async fn test_tick_skips_when_latest_trade_date_is_not_today() {
        let log = Arc::new(Mutex::new(Vec::new()));
        // 09-03 是周四，但最新交易日还停在 09-02 → 当日行情尚未落库
        let s = scheduler_with(
            FakeProbe::new(Some("2026-09-02")),
            recording_job("j", log.clone(), JobRun::ok("")),
        );

        assert_eq!(s.tick_on("2026-09-03").await, 0);
        assert!(log.lock().await.is_empty());
    }

    #[tokio::test]
    async fn test_tick_runs_when_latest_trade_date_is_today() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let s = scheduler_with(
            FakeProbe::new(Some("2026-09-03")),
            recording_job("j", log.clone(), JobRun::ok("done")),
        );

        assert_eq!(s.tick_on("2026-09-03").await, 1);
        assert_eq!(*log.lock().await, vec!["2026-09-03".to_string()]);
    }

    #[tokio::test]
    async fn test_tick_skips_when_probe_returns_nothing() {
        // 探测失败按「无数据」处理，下一轮再试，不该当成错误去跑任务
        let log = Arc::new(Mutex::new(Vec::new()));
        let s = scheduler_with(
            FakeProbe::new(None),
            recording_job("j", log.clone(), JobRun::ok("")),
        );

        assert_eq!(s.tick_on("2026-09-03").await, 0);
        assert!(log.lock().await.is_empty());
    }

    #[tokio::test]
    async fn test_skipped_job_does_not_count_as_run() {
        // 幂等：任务体查到已归档就返回 Skipped，引擎不应把它算作执行成功
        let log = Arc::new(Mutex::new(Vec::new()));
        let s = scheduler_with(
            FakeProbe::new(Some("2026-09-03")),
            recording_job("j", log.clone(), JobRun::skipped("已归档")),
        );

        assert_eq!(s.tick_on("2026-09-03").await, 0);
        // 任务体确实被调用过一次（它自己决定跳过），只是不计入成功数
        assert_eq!(log.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn test_slow_job_is_not_started_twice() {
        // 任务耗时数十秒时，下一轮 tick 不应再起一个同样的任务
        let started = Arc::new(Mutex::new(0usize));
        let mut s = Scheduler::new(
            SchedulerConfig::default(),
            Arc::new(FakeProbe::new(Some("2026-09-03"))),
        );
        let started2 = started.clone();
        s.register(FnJob::new("slow", move |_date: String| {
            let started = started2.clone();
            Box::pin(async move {
                *started.lock().await += 1;
                tokio::time::sleep(Duration::from_millis(200)).await;
                JobRun::ok("ok")
            })
        }));

        // 两轮 tick 并发：第二轮应因「上一轮尚未结束」直接跳过
        let (a, b) = futures::join!(s.tick_on("2026-09-03"), s.tick_on("2026-09-03"));
        assert_eq!(a + b, 1, "同一任务同一日期只能成功执行一次");
        assert_eq!(*started.lock().await, 1, "任务体只应被启动一次");
    }

    #[tokio::test]
    async fn test_catch_up_visits_workdays_oldest_first() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut cfg = SchedulerConfig::default();
        cfg.catchup_days = 3;
        cfg.catchup_gap = Duration::from_millis(0);
        let mut s = Scheduler::new(cfg, Arc::new(FakeProbe::new(None)));
        s.register(recording_job("j", log.clone(), JobRun::ok("")));

        s.catch_up().await;

        let dates = log.lock().await.clone();
        assert_eq!(dates.len(), 3);
        // 升序：先补历史，最后处理今天
        assert!(
            dates.windows(2).all(|w| w[0] < w[1]),
            "应按日期升序补跑: {dates:?}"
        );
        assert!(
            dates.iter().all(|d| !is_weekend(d)),
            "补跑清单不应含周末: {dates:?}"
        );
    }

    #[tokio::test]
    async fn test_catch_up_skips_jobs_that_opt_out() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut cfg = SchedulerConfig::default();
        cfg.catchup_days = 3;
        cfg.catchup_gap = Duration::from_millis(0);
        let mut s = Scheduler::new(cfg, Arc::new(FakeProbe::new(None)));
        s.register(recording_job("once-a-day", log.clone(), JobRun::ok("")).skip_catchup());

        s.catch_up().await;
        assert!(
            log.lock().await.is_empty(),
            "每日一次的维护任务不该在补跑里重复执行"
        );
    }

    #[tokio::test]
    async fn test_catch_up_disabled_by_zero_days() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut cfg = SchedulerConfig::default();
        cfg.catchup_days = 0;
        let mut s = Scheduler::new(cfg, Arc::new(FakeProbe::new(None)));
        s.register(recording_job("j", log.clone(), JobRun::ok("")));

        s.catch_up().await;
        assert!(log.lock().await.is_empty());
    }

    #[test]
    fn test_is_weekend() {
        // 2026-09-05 周六、09-06 周日、09-03 周四、09-04 周五
        assert!(is_weekend("2026-09-05"));
        assert!(is_weekend("2026-09-06"));
        assert!(!is_weekend("2026-09-03"));
        assert!(!is_weekend("2026-09-04"));
    }

    #[test]
    fn test_recent_workdays_count_and_order() {
        let days = recent_workdays(5);
        assert_eq!(days.len(), 5);
        assert!(days.windows(2).all(|w| w[0] < w[1]));
        assert!(days.iter().all(|d| !is_weekend(d)));
    }
}
