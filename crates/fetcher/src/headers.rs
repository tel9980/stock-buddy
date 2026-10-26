//! 反屏蔽请求头：UA 轮换池 + Referer + keep-alive。
//!
//! 设计参考 quant-desktop headers.rs 的通用做法（UA 池轮换），独立实现。
//! 刻意不加 Accept / Accept-Encoding —— 部分行情 JSON 接口对浏览器式
//! 内容协商敏感，加了反而可能破坏解析。

use std::sync::atomic::{AtomicUsize, Ordering};

/// 真实浏览器 UA 池（Windows Chrome/Edge/Firefox）
const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:133.0) Gecko/20100101 Firefox/133.0",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
];

static UA_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// 轮换取一个 UA。
pub fn pick_user_agent() -> &'static str {
    let i = UA_COUNTER.fetch_add(1, Ordering::Relaxed) % USER_AGENTS.len();
    USER_AGENTS[i]
}

/// 给请求挂上轮换 UA + Referer + keep-alive。
pub fn with_browser_headers(
    req: reqwest::RequestBuilder,
    referer: &str,
) -> reqwest::RequestBuilder {
    req.header("User-Agent", pick_user_agent())
        .header("Referer", referer)
        .header("Connection", "keep-alive")
}

/// 全仓共享的 reqwest Client（OnceLock 单例）。
///
/// 连接池 / TLS 配置 / 超时由所有数据源共享；`Client::clone()` 是浅拷贝。
/// 各 fetcher 不再各自 `Client::builder()`，避免连接池碎片化。
pub fn shared_client() -> &'static reqwest::Client {
    use std::sync::OnceLock;
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(3))
            .timeout(std::time::Duration::from_secs(10))
            .pool_max_idle_per_host(10)
            .build()
            .expect("Failed to build shared reqwest Client")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ua_rotates() {
        let a = pick_user_agent();
        let b = pick_user_agent();
        assert_ne!(a, b);
    }
}
