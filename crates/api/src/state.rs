use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use analyzer::AiService;

pub struct AppState {
    pub listen_addr: String,
    pub fetcher: Arc<dyn fetcher::DataFetcher>,
    pub db: storage::Database,
    pub ai: Arc<AiService>,
    /// 导出件留存目录（情绪日报 HTML 等）
    pub report_dir: PathBuf,
    /// 正在生成日报的日期 → 引用计数
    ///
    /// 定时任务与用户手动请求各跑一次就是两轮全市场抓取（各约 5,600 次请求），
    /// 必然触发行情源限流。这里只做**单向避让**：手动请求（用户主动在页面上等，
    /// 优先级更高）标记日期，定时任务发现被占用就跳过本轮；手动请求之间不互斥，
    /// 保持既有行为不变。
    ///
    /// 用引用计数而非 bool：两个手动请求并发时，先结束的那个不该提前解开标记。
    in_flight: Mutex<HashMap<String, usize>>,
}

impl AppState {
    pub fn new(
        listen_addr: String,
        fetcher: Arc<dyn fetcher::DataFetcher>,
        db: storage::Database,
        ai: AiService,
        report_dir: PathBuf,
    ) -> Self {
        Self {
            listen_addr,
            fetcher,
            db,
            ai: Arc::new(ai),
            report_dir,
            in_flight: Mutex::new(HashMap::new()),
        }
    }

    /// 标记某日期正在生成日报，返回守卫；守卫离开作用域时自动解除
    pub fn begin_date(&self, date: &str) -> DateGuard<'_> {
        {
            let mut m = self.lock();
            *m.entry(date.to_string()).or_insert(0) += 1;
        }
        DateGuard {
            state: self,
            date: date.to_string(),
        }
    }

    /// 该日期是否正在被生成（供定时任务避让）
    pub fn date_in_flight(&self, date: &str) -> bool {
        self.lock().get(date).copied().unwrap_or(0) > 0
    }

    /// 取锁。
    ///
    /// 持有它的线程 panic 会让锁中毒，但这里的临界区只是几次 HashMap 增删，
    /// 数据本身不会处于半更新状态，故中毒后照旧使用而不是让它扩散成 panic。
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, usize>> {
        self.in_flight.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// 日期占用守卫，Drop 时解除标记
pub struct DateGuard<'a> {
    state: &'a AppState,
    date: String,
}

impl Drop for DateGuard<'_> {
    fn drop(&mut self) {
        let mut m = self.state.lock();
        match m.get_mut(&self.date) {
            // 计数减到 0 才真正移除，否则只递减
            Some(n) if *n > 1 => *n -= 1,
            _ => {
                m.remove(&self.date);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> AppState {
        AppState {
            listen_addr: String::new(),
            fetcher: Arc::new(fetcher::EastmoneyFetcher::new()),
            db: futures::executor::block_on(storage::Database::in_memory()).unwrap(),
            ai: Arc::new(analyzer::AiService::new(
                analyzer::ai::LlmClient::from_env(),
                Arc::new(fetcher::EastmoneyFetcher::new()),
            )),
            report_dir: PathBuf::from("data/reports"),
            in_flight: Mutex::new(HashMap::new()),
        }
    }

    // 建库要 Tokio 上下文（sqlx 的连接池依赖它），故这几个测试都标 #[tokio::test]
    #[tokio::test]
    async fn test_date_guard_releases_on_drop() {
        let s = state();
        assert!(!s.date_in_flight("2026-09-03"));
        {
            let _g = s.begin_date("2026-09-03");
            assert!(s.date_in_flight("2026-09-03"));
        }
        assert!(!s.date_in_flight("2026-09-03"), "守卫 Drop 后应自动解除");
    }

    #[tokio::test]
    async fn test_concurrent_guards_keep_date_marked_until_last_drops() {
        let s = state();
        let a = s.begin_date("2026-09-03");
        let b = s.begin_date("2026-09-03");
        drop(a);
        // 引用计数：还有一个人持有，标记不能提前解开
        assert!(s.date_in_flight("2026-09-03"));
        drop(b);
        assert!(!s.date_in_flight("2026-09-03"));
    }

    #[tokio::test]
    async fn test_different_dates_are_independent() {
        let s = state();
        let _g = s.begin_date("2026-09-03");
        assert!(s.date_in_flight("2026-09-03"));
        assert!(!s.date_in_flight("2026-09-02"));
    }
}
