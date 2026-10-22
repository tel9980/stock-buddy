//! 行情主源的熔断（circuit breaker）
//!
//! ## 解决的问题
//!
//! 日线抓取是「腾讯主源 + 新浪兜底」的双源结构。主源被 WAF 拦截时（实测
//! `web.ifzq.gtimg.cn` 返回 HTTP 501 + `waf.tencent.com/501page.html`，与
//! User-Agent 无关，6 分钟以上不恢复），**并不会因为返回错误码就停止被访问**：
//! 每只股票仍然先探一次腾讯，5642 只成分股 = 5642 次注定失败的请求。
//!
//! 长连接实测 45–66ms/次，冷启动一次白烧约 15.6s（占 34.8s 的 45%）；
//! 更糟的是请求量翻倍，反过来更容易把兜底的新浪也拖进限流。
//!
//! ## 关键设计：只统计「源级故障」
//!
//! | 判定 | 含义 | 是否计入熔断 |
//! |---|---|---|
//! | 网络错误 / 响应体不是合法 JSON（WAF 页面） | 源挂了 | ✅ 计入 |
//! | 合法 JSON 但无该标的、或历史窗口不够 | 个股级缺失 | ❌ 不计，且清零计数 |
//!
//! 把个股级缺失也算进去是错的：北交所那几只未上市的标的就能把整个主源熔断掉，
//! 而主源其实是好的。那会**静默改变数据口径**——腾讯是前复权、新浪是不复权，
//! 用户看到的 K 线会毫无征兆地换一套价格。
//!
//! 反过来，一次成功的解析就是「源活着」的强证据，因此直接把计数清零。
//!
//! ## 为什么不做半开（half-open）状态
//!
//! 冷却结束后直接放行并清零计数：若主源仍故障，只需再攒满阈值次就会重新熔断。
//! 并发度是 24、阈值是 20，也就是一批请求就能重新熔断，持续故障期间每 5 分钟
//! 最多白跑 20 次请求——相比半开状态机那套 bookkeeping，这点开销不值得换来
//! 额外复杂度。

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 默认阈值：连续这么多次**源级故障**后熔断
///
/// 20 这个数字是这么定的：并发度 24（见 `sentiment::BAR_CONCURRENCY`），
/// 阈值必须小于一个并发批次，否则「主源已挂」这件事要等第二批请求才被确认，
/// 白白多烧一批（约 24 × 50ms = 1.2s）；取 20 则第一批内即可熔断。
/// 同时它远大于一次网络抖动能连累的请求数（个位数），不会因为瞬时丢包就切源。
pub const DEFAULT_THRESHOLD: u32 = 20;

/// 默认冷却时长
///
/// WAF 拦截实测 6 分钟以上不恢复，取 5 分钟：既有机会在恢复后自动切回前复权口径，
/// 又不至于在持续故障期间频繁放流量试探。
pub const DEFAULT_COOLDOWN: Duration = Duration::from_secs(300);

#[derive(Debug)]
struct BreakerState {
    /// 连续源级故障次数
    consecutive_failures: u32,
    /// 熔断开始时刻。`None` 表示当前未熔断（含冷却已结束、正在放流量试探）
    opened_at: Option<Instant>,
    /// 是否曾经熔断过。用于在主源恢复时打一条日志——冷却结束那一刻我们只知道
    /// 「可以再试试」，还不知道「已经好了」。
    ever_opened: bool,
}

/// 单个行情源的熔断状态
///
/// 内部用 `std::sync::Mutex` 而非 `tokio::sync::Mutex`：临界区只有几次整数运算，
/// 不存在 `.await`，用同步锁更轻；并且用 `into_inner()` 容忍中毒——上一个持锁者
/// panic 不该让整个行情源永久不可用。
#[derive(Debug)]
pub struct SourceBreaker {
    state: Mutex<BreakerState>,
    threshold: u32,
    cooldown: Duration,
}

impl SourceBreaker {
    pub fn new() -> Self {
        Self::with(DEFAULT_THRESHOLD, DEFAULT_COOLDOWN)
    }

    /// 按自定义参数构造（单测用短冷却、小阈值，避免依赖真实时钟与外网）
    pub fn with(threshold: u32, cooldown: Duration) -> Self {
        Self {
            state: Mutex::new(BreakerState {
                consecutive_failures: 0,
                opened_at: None,
                ever_opened: false,
            }),
            threshold,
            cooldown,
        }
    }

    /// 当前是否处于熔断期（调用方应跳过该源）
    ///
    /// 冷却已结束时**顺带复位**并返回 false，让流量重新进来试探。这里承担状态
    /// 转移而不是另开一个 `maybe_reset`：调用点只有一个，分散反而容易出现
    /// 「忘了复位导致永久熔断」。
    pub fn is_open(&self) -> bool {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        match st.opened_at {
            Some(at) if at.elapsed() < self.cooldown => true,
            // 冷却结束：复位成「未熔断、计数归零」，本次调用放行去试探
            Some(_) => {
                st.opened_at = None;
                st.consecutive_failures = 0;
                false
            }
            None => false,
        }
    }

    /// 记录一次**源级故障**（网络错误 / 响应体不是合法 JSON）
    pub fn record_failure(&self) {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        // 熔断期内不该有请求打进来，保险起见不重复累加，避免计数无上限增长
        if st.opened_at.is_some() {
            return;
        }
        st.consecutive_failures = st.consecutive_failures.saturating_add(1);
        if st.consecutive_failures >= self.threshold {
            st.opened_at = Some(Instant::now());
            st.ever_opened = true;
            tracing::warn!(
                threshold = self.threshold,
                cooldown_secs = self.cooldown.as_secs(),
                "行情主源连续源级故障达到阈值，已熔断，冷却期内日线全部走兜底源"
            );
        }
    }

    /// 记录一次**成功解析**（响应体是合法 JSON，哪怕里面没有该标的数据）
    pub fn record_success(&self) {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.consecutive_failures = 0;
        if st.ever_opened {
            st.ever_opened = false;
            tracing::warn!("行情主源已恢复，日线恢复使用主源口径");
        }
    }
}

impl Default for SourceBreaker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 速通配置：阈值 3、冷却 120ms，单测不依赖真实时钟
    fn fast() -> SourceBreaker {
        SourceBreaker::with(3, Duration::from_millis(120))
    }

    #[test]
    fn 未达阈值不熔断() {
        let b = fast();
        b.record_failure();
        b.record_failure();
        assert!(!b.is_open(), "2 次故障未达阈值 3，不应熔断");
    }

    #[test]
    fn 达到阈值即熔断() {
        let b = fast();
        for _ in 0..3 {
            b.record_failure();
        }
        assert!(b.is_open(), "连续 3 次源级故障应熔断");
    }

    #[test]
    fn 成功会清零计数() {
        let b = fast();
        b.record_failure();
        b.record_failure();
        // 一次成功解析就把计数清零：它证明源是活的
        b.record_success();
        b.record_failure();
        b.record_failure();
        assert!(!b.is_open(), "成功应清零计数，2+2 次故障跨过成功点不该累加熔断");
    }

    #[test]
    fn 冷却结束后放行试探() {
        let b = fast();
        for _ in 0..3 {
            b.record_failure();
        }
        assert!(b.is_open());
        std::thread::sleep(Duration::from_millis(150));
        assert!(!b.is_open(), "冷却结束后应放行试探");
    }

    #[test]
    fn 试探仍失败会重新熔断() {
        let b = fast();
        for _ in 0..3 {
            b.record_failure();
        }
        std::thread::sleep(Duration::from_millis(150));
        assert!(!b.is_open(), "冷却结束后进入试探");
        // 试探阶段再攒满阈值即重新熔断
        for _ in 0..3 {
            b.record_failure();
        }
        assert!(b.is_open(), "试探失败应重新熔断");
    }

    #[test]
    fn 熔断期内不再累加计数() {
        let b = fast();
        for _ in 0..10 {
            b.record_failure();
        }
        let st = b.state.lock().unwrap();
        assert_eq!(
            st.consecutive_failures, 3,
            "熔断后继续上报故障不应让计数无限增长"
        );
    }

    #[test]
    fn 并发上报故障最终熔断() {
        use std::sync::Arc;
        let b = Arc::new(SourceBreaker::with(50, Duration::from_secs(60)));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let b = Arc::clone(&b);
            handles.push(std::thread::spawn(move || {
                for _ in 0..10 {
                    b.record_failure();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert!(b.is_open(), "8 线程 × 10 次故障应触发阈值为 50 的熔断");
    }
}
