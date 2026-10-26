//! 东财 `push2` 行情集群的多节点故障切换 + 整段不可达时的静默降噪。
//!
//! ## 为什么需要
//!
//! `push2` 集群有多个等价节点（`push2` / `push2delay` / `1.push2` …），同一接口
//! 在不同节点上的可用性并不同步。实测（2026-10-26）：出口被上游风控后，
//! `clist/get` 会返回**空响应**（TCP 连上但服务端不回包 → reqwest "error sending
//! request"），而同一时刻 `stock/get` 却能正常返回。只钉一个域名，就意味着这一个
//! 接口挂掉时整块功能（板块快照 / 涨幅榜 / 资金流）一起挂。
//!
//! ## 策略
//!
//! 1. 按 `HOSTS` 顺序尝试，任一节点成功即返回（成功节点提到队首，冷启动自愈）。
//! 2. 全部失败视为「集群整段不可达」：进入 `QUIET_FOR` 静默期，期间直接跳过
//!    远程调用（由调用方走备选源），并只打一条 WARN——否则一次刷新就会刷出
//!    几十条同样的 ERROR 把日志淹掉。
//! 3. 静默期结束后自动重试一次，成功即退出静默，失败则重新计时。

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use appcore::{AppError, AppResult};
use reqwest::Client;

/// 等价节点，顺序即优先级。全都在同一套 `api/qt/...` 路径下。
pub const HOSTS: &[&str] = &[
    "https://push2.eastmoney.com",
    "https://push2delay.eastmoney.com",
    "https://1.push2.eastmoney.com",
    "https://7.push2.eastmoney.com",
];

/// 集群整段不可达后的静默时长。
///
/// 取 90s：刷新页面通常间隔几十秒，太短会重新开始刷屏；太长则域名恢复后
/// 功能迟迟不回来。到期后自动探测一次，所以不会卡死。
const QUIET_FOR: Duration = Duration::from_secs(90);

static PREFERRED: AtomicUsize = AtomicUsize::new(0);
/// 静默截止时间（单调时钟，以进程启动为基准的毫秒数），0 表示未静默
static QUIET_UNTIL_MS: AtomicU64 = AtomicU64::new(0);
static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

fn now_ms() -> u64 {
    EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// 集群是否处于静默期（调用方可据此跳过远程调用直接走备选源）
pub fn is_quiet() -> bool {
    let until = QUIET_UNTIL_MS.load(Ordering::Relaxed);
    until != 0 && now_ms() < until
}

fn enter_quiet() {
    let until = now_ms() + QUIET_FOR.as_millis() as u64;
    let prev = QUIET_UNTIL_MS.swap(until, Ordering::Relaxed);
    // 只在「刚进入」时告警一次，静默期内重复失败不再刷日志
    if prev == 0 || prev <= now_ms() {
        tracing::warn!(
            seconds = QUIET_FOR.as_secs(),
            "东财 push2 集群整段不可达（所有节点均无响应），{}s 内改用备选数据源；\
             通常是出口 IP 被风控限流，稍后会自动重试",
            QUIET_FOR.as_secs()
        );
    }
}

fn exit_quiet() {
    if QUIET_UNTIL_MS.swap(0, Ordering::Relaxed) != 0 {
        tracing::info!("东财 push2 集群已恢复，切回主源");
    }
}

/// 取得板块列表接口的完整 URL 列表（首选节点在前）。
fn candidate_urls(path_and_query: &str) -> Vec<String> {
    let start = PREFERRED.load(Ordering::Relaxed) % HOSTS.len();
    (0..HOSTS.len())
        .map(|i| format!("{}{}", HOSTS[(start + i) % HOSTS.len()], path_and_query))
        .collect()
}

/// 依次尝试各节点取一段 JSON 文本。
///
/// * `path_and_query` 形如 `/api/qt/clist/get?pn=1&...`（不含域名）
/// * 全部失败时返回最后一次的错误，并进入静默期
pub async fn get_text_any_node(client: &Client, path_and_query: &str) -> AppResult<String> {
    let mut last_err: Option<String> = None;

    for (idx, url) in candidate_urls(path_and_query).iter().enumerate() {
        match client.get(url).send().await {
            Ok(resp) => match resp.error_for_status() {
                // 空响应体也算失败：风控节点会回 200 + 空 body，直接吞掉会一路
                // 变成「解析失败」的假象，让人误以为是字段变了
                Ok(resp) => match resp.text().await {
                    Ok(body) if !body.trim().is_empty() => {
                        // 命中的节点提到队首，后续请求优先用它
                        let hit = (PREFERRED.load(Ordering::Relaxed) + idx) % HOSTS.len();
                        PREFERRED.store(hit, Ordering::Relaxed);
                        exit_quiet();
                        return Ok(body);
                    }
                    // 200 + 空 body：风控节点的典型表现，换下一个节点
                    Ok(_) => last_err = Some(format!("{url} 返回空响应")),
                    Err(e) => last_err = Some(format!("{url}: {e}")),
                },
                Err(e) => last_err = Some(format!("{url}: {e}")),
            },
            Err(e) => last_err = Some(format!("{url}: {e}")),
        }
    }

    enter_quiet();
    Err(AppError::Network(format!(
        "push2 集群无可用节点: {}",
        last_err.unwrap_or_else(|| "全部节点失败".to_string())
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_urls_rotates_and_covers_all_hosts() {
        PREFERRED.store(2, Ordering::Relaxed);
        let urls = candidate_urls("/api/qt/clist/get?pn=1");
        assert_eq!(urls.len(), HOSTS.len());
        assert!(urls[0].starts_with(HOSTS[2]));
        // 全部节点都被覆盖，不重复也不缺
        let uniq: std::collections::HashSet<_> = urls.iter().map(|u| u.split("/api").next().unwrap()).collect();
        assert_eq!(uniq.len(), HOSTS.len());
        PREFERRED.store(0, Ordering::Relaxed);
    }

    #[test]
    fn quiet_window_opens_and_expires() {
        exit_quiet();
        assert!(!is_quiet());
        enter_quiet();
        assert!(is_quiet());
        exit_quiet();
        assert!(!is_quiet());
    }

    /// 联调用：需要外网。`cargo test -p fetcher live_probe -- --ignored --nocapture`
    ///
    /// 逐个节点打印可用性，方便判断是「单节点抽风」还是「整段被风控」。
    #[tokio::test]
    #[ignore]
    async fn live_probe_any_node() {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(6))
            .build()
            .unwrap();
        let path = "/api/qt/clist/get?pn=1&pz=3&po=1&np=1&fltt=2&invt=2&fid=f3&fs=m:90+t:2&fields=f12,f14,f3";
        for host in HOSTS {
            let url = format!("{host}{path}");
            let r = client.get(&url).send().await;
            match r {
                Ok(resp) => {
                    let code = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    println!("{host:35} HTTP {code} body_len={}", body.len());
                    if !body.is_empty() {
                        println!("    {}", &body[..body.len().min(120)]);
                    }
                }
                Err(e) => println!("{host:35} ERR {e}"),
            }
        }
        println!("---- 汇总调用 ----");
        match get_text_any_node(&client, path).await {
            Ok(b) => println!("成功，body_len={}", b.len()),
            Err(e) => println!("全部失败: {e}"),
        }
        println!("is_quiet = {}", is_quiet());
    }
}
