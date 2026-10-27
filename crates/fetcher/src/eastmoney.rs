use async_trait::async_trait;
use appcore::models::*;
use appcore::{AppError, AppResult};
use crate::breaker::SourceBreaker;
use crate::traits::DataFetcher;
use futures::stream::{self, StreamExt};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::future::Future;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::traits::KlineStatus;

// 单券行情走本常量；**列表类接口**（clist / ulist）统一走 crate::push2 的多节点
// 切换，因为 push2 集群各节点可用性并不同步，且会在出口被风控时整体返回空响应。
const EM_BASE: &str = "http://push2delay.eastmoney.com/api/qt/stock";

/// 分页抓取的并发度
///
/// ## 为什么是 4
///
/// 依据是**服务端是否开始排队**，不只是墙钟时间。单页请求实测（2026-09-03 本机）：
///
/// | 并发 | 1 | 3 | 4 | 6 | 8 |
/// |---|---|---|---|---|---|
/// | 全市场快表 11 页 | 2.05s | 0.65s | **0.55s** | 0.45s | 0.64s |
/// | 单页 p50 | 0.16s | 0.16s | **0.16s** | 0.19s | 0.30s |
///
/// 并发 ≤4 时单页 p50 与串行持平，说明服务端没有排队；并发 6 收益已明显扁平；
/// 并发 8 时 p50 涨到 0.30s 且墙钟反而变慢——那不是加速，是把等待搬到东财那边去了。
///
/// ## 端到端效果（同一天 2026-09-03，各 13 轮 `refresh=true`）
///
/// | | 串行 (conc=1) | 并发 4 |
/// |---|---|---|
/// | 中位 | 2494ms | **1720ms** |
/// | P90 | 7366ms | **2171ms** |
/// | 最坏 | 18443ms | **2390ms** |
/// | 标准差 | 4521ms | **259ms** |
///
/// 最大的收益其实在长尾而不在中位数：串行时有两轮撞上 7.4s / 18.4s（瞬时失败触发
/// 整条快路径重试），并发后最坏只有 2.4s。暴露窗口越短，撞上瞬时故障的概率越低。
///
/// ## 安全性
///
/// 并发 4 × 25 轮（275 次请求）返回的内容与串行**逐字节一致**（按页号展开后 sha256
/// 相同）：两个接口都带 `sortColumns`，单页内容本身是确定的，并发只影响到达顺序。
/// 端到端另做 26 轮比对，`boards` / `breadth` / `themes` 指纹全部相同。
const PAGE_CONCURRENCY: usize = 4;

/// 并发抓取一批分页，返回**与入参顺序一致**的响应体
///
/// 两处容易踩的点：
///
/// 1. **必须按页号重排。** `buffer_unordered` 的完成顺序由网络决定，而分页顺序
///    参与下游的行业等权聚合与按代码去重——乱序会让同一份数据产出不同的结果，
///    表现为日报里行业排名莫名其妙地跳动。
/// 2. **任一页失败即整体失败。** 宁可让快路径整体回落到逐股聚合，也不要拿半份
///    数据算宽度与涨跌幅：那会静默偏小，且看不出是数据问题还是行情问题。
async fn fetch_pages_concurrent<F, Fut>(urls: Vec<String>, fetch: F) -> AppResult<Vec<String>>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = AppResult<String>> + Send,
{
    let mut ordered: Vec<(usize, AppResult<String>)> = stream::iter(urls)
        .enumerate()
        .map(|(idx, url)| {
            let fut = fetch(url);
            async move { (idx, fut.await) }
        })
        .buffer_unordered(PAGE_CONCURRENCY)
        .collect()
        .await;

    ordered.sort_by_key(|(idx, _)| *idx);
    ordered.into_iter().map(|(_, body)| body).collect()
}

/// 东财板块列表的一个板块快照（最新交易日）
///
/// 金额单位：万元（与项目其余模型一致）。
#[derive(Debug, Clone)]
pub struct BoardSnapshot {
    /// 板块名称，与 `fetch_industry_boards` 返回的二级行业名一一对应
    pub name: String,
    /// 东财板块代码，如 `BK0428`
    pub bk_code: String,
    /// 官方涨跌幅 (%)
    pub change_pct: f64,
    /// 板块成交额（万元）
    pub amount: f64,
    pub up_count: u32,
    pub down_count: u32,
    pub flat_count: u32,
    pub leader: Option<String>,
    pub leader_code: Option<String>,
    pub leader_pct: f64,
}

/// 个股最新交易日快照（东财全市场快表）
#[derive(Debug, Clone)]
pub struct MarketBar {
    pub code: String,
    pub name: String,
    pub close: f64,
    /// 涨跌幅 (%)
    pub pct_change: f64,
}

/// 腾讯主源一次请求的结果，区分「这只股票没数据」和「源挂了」
///
/// 这个区分是熔断能安全工作的前提，见 `SourceBreaker` 的说明。
/// 判据落在**响应体是否为合法 JSON**上：WAF 拦截页是 HTML，`serde_json::from_str`
/// 必然失败；而 `resp.code != 0`、节点缺失、行为空都属于合法 JSON，是个股级缺失。
enum TencentOutcome {
    /// 成功解析。可能为空——无此标的或历史窗口为空，属个股级缺失
    Ok(Vec<Kline>),
    /// 源级故障：网络层报错，或响应体不是这份 JSON
    SourceDown,
}

/// 东方财富数据采集器
///
/// `tencent_breaker` 是主源（腾讯日线）的熔断状态。它被 `api` 层包成单个 `Arc`
/// 全局共享（见 `api::app`），因此状态可跨请求保留：一次冷启动里第一批请求发现
/// 主源挂了，后面几千只股票就都不再白跑一趟。
pub struct EastmoneyFetcher {
    client: Client,
    tencent_breaker: SourceBreaker,
    /// 涨停复盘/龙虎榜的第三路源（同花顺 fuyao）。
    ///
    /// 东财 push2 集群一旦被风控，涨停池/炸板池/龙虎榜会同时空白；新浪没有
    /// 这几块数据，所以单独挂一路。未配 `FUYAO_API_KEY` 时本字段仍在但
    /// `is_configured()` 为 false，调用处会跳过。
    fuyao: crate::fuyao::FuyaoFetcher,
}

/// 东财 JSON 字段取数值（兼容数字与字符串两种形态）
fn em_num(v: &Option<serde_json::Value>) -> f64 {
    match v {
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(serde_json::Value::String(s)) => s.trim().parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// 东财 JSON 字段取字符串，空串视为缺失
fn em_str_opt(v: &Option<serde_json::Value>) -> Option<String> {
    match v {
        Some(serde_json::Value::String(s)) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Some(serde_json::Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

fn em_str(v: &Option<serde_json::Value>) -> String {
    em_str_opt(v).unwrap_or_default()
}

impl EastmoneyFetcher {
    /// 备选源：新浪行业/概念板块 → 映射成 `SectorRank`
    ///
    /// 新浪不给板块主力净流入，`net_amount` 只能留空（下游按 Option 处理）。
    async fn fetch_sector_ranks_sina(&self, sector_type: &str) -> AppResult<Vec<SectorRank>> {
        let boards = match sector_type {
            "concept" => crate::sina_boards::fetch_concept_boards(&self.client).await?,
            _ => crate::sina_boards::fetch_industry_boards(&self.client).await?,
        };
        tracing::info!(count = boards.len(), %sector_type, "板块涨幅榜改用新浪备选源");

        let mut ranks: Vec<SectorRank> = boards
            .into_iter()
            .map(|b| SectorRank {
                code: b.code,
                name: b.name,
                sector_type: sector_type.to_string(),
                change_pct: b.change_pct,
                leader_stock: b.leader_name,
                leader_code: b.leader_code,
                leader_pct: b.leader_pct,
                net_amount: None,
                // 新浪接口不提供板块成分股涨跌家数
                up_count: None,
                down_count: None,
            })
            .collect();
        // 与东财口径一致：按涨跌幅降序
        ranks.sort_by(|a, b| {
            b.change_pct
                .partial_cmp(&a.change_pct)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(ranks)
    }

    /// 东财主源实现（多节点）
    async fn fetch_sector_ranks_em_inner(&self, sector_type: &str) -> AppResult<Vec<SectorRank>> {
        // m:90+t:2 行业板块, m:90+t:3 概念板块
        let fs = match sector_type {
            "industry" => "m:90+t:2",
            "concept" => "m:90+t:3",
            _ => "m:90+t:2",
        };

        // f140 = 领涨股代码，别漏：少了它就只能拿 f128（名称）去当代码查财报
        let path = format!(
            "/api/qt/clist/get?pn=1&pz=100&po=1&np=1&fltt=2&invt=2&fid=f3&fs={}&fields=f12,f14,f3,f128,f140,f136,f62,f104,f105",
            fs
        );

        tracing::debug!(%sector_type, "Fetching sector ranks via push2 nodes");

        let body = crate::push2::get_text_any_node(&self.client, &path).await?;
        let resp: EmListResponse = serde_json::from_str(&body)?;

        let items = resp
            .data
            .and_then(|d| d.diff)
            .unwrap_or_default();

        let ranks: Vec<SectorRank> = items
            .iter()
            .map(|item| SectorRank {
                code: item.code.clone(),
                name: item.name.clone(),
                sector_type: sector_type.to_string(),
                change_pct: item.change_pct,
                leader_stock: if item.leader_stock.is_empty() {
                    None
                } else {
                    Some(item.leader_stock.clone())
                },
                leader_code: if item.leader_code.is_empty() {
                    None
                } else {
                    Some(item.leader_code.clone())
                },
                leader_pct: if item.leader_pct != 0.0 {
                    Some(item.leader_pct)
                } else {
                    None
                },
                net_amount: if item.net_amount != 0.0 {
                    Some(item.net_amount)
                } else {
                    None
                },
                up_count: Some(item.up_count),
                down_count: Some(item.down_count),
            })
            .collect();

        Ok(ranks)
    }

    pub fn new() -> Self {
        // 使用全仓共享 Client（连接池/超时统一管理）
        let client = crate::headers::shared_client().clone();
        Self {
            client,
            tencent_breaker: SourceBreaker::new(),
            fuyao: crate::fuyao::FuyaoFetcher::new(),
        }
    }

    /// 用指定同花顺 key 构造（免环境变量，测试/多租户用）
    pub fn with_fuyao_key(mut self, key: impl Into<String>) -> Self {
        self.fuyao = crate::fuyao::FuyaoFetcher::with_api_key(key);
        self
    }

    /// 构造 secid (市场前缀 + 代码)
    fn secid(code: &str) -> String {
        let market = Market::from_code(code);
        format!("{}.{}", market.secid_prefix(), code)
    }

    fn now_ts() -> i64 {
        // 时钟倒退（duration_since 失败）时回退 0，不要 panic
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    /// 请求东财 datacenter 报表并返回原始文本
    ///
    /// datacenter 会返回 `Content-Encoding: gzip` 的响应体（且并非合法 gzip），
    /// 直接 `.json()` 会报 "error decoding response body"。强制 identity 并在本地解析。
    async fn datacenter_text(&self, url: &str) -> AppResult<String> {
        // 源码里用 `\` 续行拼接的长 URL 会带上缩进空白；HTTP 容忍，但日志/错误信息里
        // 一大串空格很难读，这里统一压成单空格（datacenter 的 query 不含空格）。
        let url = url.split_whitespace().collect::<Vec<_>>().join(" ");
        let text = self
            .client
            .get(&url)
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .send()
            .await?
            .text()
            .await?;
        Ok(text)
    }

    /// 获取指定交易日的东财行业板块清单
    ///
    /// 这是**唯一能按历史日期**取到行业板块清单的官方接口（push2 的板块排行只有实时数据）。
    /// 返回的 `NUM` 为东财官方披露的板块成分股家数。
    pub async fn fetch_industry_boards(
        &self,
        date: &str,
    ) -> AppResult<Vec<(String, u32)>> {
        let url = format!(
            "http://datacenter.eastmoney.com/securities/api/data/v1/get?\
             reportName=RPT_VALUEINDUSTRY_DET&columns=ALL&pageNumber=1&pageSize=500&\
             source=WEB&client=WEB&filter=(TRADE_DATE%3D%27{}%27)",
            date
        );

        #[derive(Deserialize)]
        struct Resp {
            result: Option<ResultData>,
        }
        #[derive(Deserialize)]
        struct ResultData {
            data: Option<Vec<Item>>,
        }
        #[derive(Deserialize)]
        struct Item {
            #[serde(rename = "BOARD_NAME", default)]
            board_name: String,
            #[serde(rename = "NUM", default)]
            num: Option<f64>,
        }

        let text = self.datacenter_text(&url).await?;
        let resp: Resp = serde_json::from_str(&text)?;
        let items = resp.result.and_then(|r| r.data).unwrap_or_default();

        let boards: Vec<(String, u32)> = items
            .into_iter()
            .filter(|it| !it.board_name.is_empty())
            .map(|it| (it.board_name, it.num.unwrap_or(0.0) as u32))
            .collect();

        Ok(boards)
    }

    /// 获取全市场个股的东财二级行业归属
    ///
    /// 用于把个股日线聚合到行业维度。东财二级行业口径（约 5600 条）与
    /// `fetch_industry_boards` 返回的板块名称一一对应，可安全 join。
    ///
    /// 数据量约 5600 条，按 pageSize=500 分页拉取，约 12 次请求。
    ///
    /// **必须带 `sortColumns`**：datacenter 分页在未指定排序时顺序不稳定，
    /// 实测会出现「跨页重复 + 漏行」——12 页共返回 5642 行，但只有 5423 个
    /// 去重代码（219 只重复计数、另有 219 只被漏掉）。加上 `sortColumns=SECUCODE`
    /// 后同一批数据为 5642 行 / 5642 个去重代码。即便如此，仍在返回前按代码
    /// 去重（保留首次出现）作为兜底，因为重复会让等权平均被错误加权。
    pub async fn fetch_stock_industry_map(&self) -> AppResult<Vec<(String, String, String)>> {
        #[derive(Deserialize)]
        struct Resp {
            result: Option<ResultData>,
        }
        #[derive(Deserialize)]
        struct ResultData {
            pages: Option<u32>,
            data: Option<Vec<Item>>,
        }
        #[derive(Deserialize)]
        struct Item {
            #[serde(rename = "SECURITY_CODE", default)]
            code: String,
            #[serde(rename = "SECURITY_NAME_ABBR", default)]
            name: String,
            #[serde(rename = "BOARD_NAME", default)]
            board_name: String,
        }

        const PAGE_SIZE: u32 = 500;
        let mut all: Vec<(String, String, String)> = Vec::new();
        let mut page = 1u32;

        loop {
            let url = format!(
                "http://datacenter.eastmoney.com/securities/api/data/v1/get?\
                 reportName=RPT_F10_CORETHEME_BOARDTYPE&columns=ALL&pageNumber={}&pageSize={}&\
                 source=WEB&client=WEB&filter=(BOARD_LEVEL%3D2)&\
                 sortColumns=SECUCODE&sortTypes=1",
                page, PAGE_SIZE
            );

            let text = self.datacenter_text(&url).await?;
            let resp: Resp = serde_json::from_str(&text)?;

            let (pages, items) = match resp.result {
                Some(r) => (r.pages.unwrap_or(1), r.data.unwrap_or_default()),
                None => break,
            };

            let got = items.len();
            for it in items {
                if !it.code.is_empty() && !it.board_name.is_empty() {
                    all.push((it.code, it.name, it.board_name));
                }
            }

            // 接口无更多数据，或已取完最后一页
            if got < PAGE_SIZE as usize || page >= pages {
                break;
            }
            page += 1;

            // 简单限流，避免触发 datacenter 的频率限制
            tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        }

        Ok(dedup_by_code(all))
    }

    /// 板块实时快照
    ///
    /// 来自东财 `push2` 集群的板块列表接口（多节点自动切换，见 [`crate::push2`]）。
    /// 这是目前**唯一能同时取到官方涨跌幅、成交额、涨跌家数与领涨股**的通道。
    ///
    /// **只有最新交易日的快照，没有历史数据。** 东财历史 K 线节点 `push2his`（含所有
    /// 编号子域）在本机不可达，`push2` 集群又完全不提供 K 线，所以历史交易日只能
    /// 回落到成分股日线等权聚合。调用方必须先用 `fetch_latest_trade_date` 做日期守卫，
    /// 否则盘中查询会拿到尚未收盘的当日快照。
    ///
    /// 东财整段不可达时（出口被风控，返回空响应）自动回落到新浪板块接口：
    /// 涨跌幅 / 成交额 / 领涨股口径一致，但**没有成分股涨跌家数**（填 0）。
    pub async fn fetch_board_snapshots(&self) -> AppResult<Vec<BoardSnapshot>> {
        // 集群处于静默期时不必再试，省一轮必然失败的请求
        if !crate::push2::is_quiet() {
            match self.fetch_board_snapshots_em().await {
                Ok(v) if !v.is_empty() => return Ok(v),
                Ok(_) => tracing::debug!("东财板块快照返回空表，改用备选源"),
                Err(e) => tracing::debug!(error = %e, "东财板块快照失败，改用备选源"),
            }
        }
        self.fetch_board_snapshots_sina().await
    }

    /// 备选源：新浪行业板块 → 映射成 `BoardSnapshot`
    async fn fetch_board_snapshots_sina(&self) -> AppResult<Vec<BoardSnapshot>> {
        let boards = crate::sina_boards::fetch_industry_boards(&self.client).await?;
        tracing::info!(count = boards.len(), "板块快照改用新浪备选源");
        Ok(boards
            .into_iter()
            .map(|b| BoardSnapshot {
                name: b.name,
                bk_code: b.code,
                change_pct: b.change_pct,
                // 项目口径是万元，新浪给的是元
                amount: b.amount_yuan / 10_000.0,
                // 新浪不给涨跌家数，只能置 0（下游不做强校验）
                up_count: 0,
                down_count: 0,
                flat_count: 0,
                leader: b.leader_name,
                leader_code: b.leader_code,
                leader_pct: b.leader_pct.unwrap_or(0.0),
            })
            .collect())
    }

    /// 东财主源实现（多节点）
    async fn fetch_board_snapshots_em(&self) -> AppResult<Vec<BoardSnapshot>> {
        #[derive(Deserialize)]
        struct Resp {
            data: Option<Data>,
        }
        #[derive(Deserialize)]
        struct Data {
            total: Option<u32>,
            diff: Option<Vec<Item>>,
        }
        /// 字段统一用 `Value` 接收：东财同一字段在不同板块上可能返回数字或字符串
        /// （如领涨股代码 `f140` 有时是 `"300413"` 有时是 `300413`），直接声明成
        /// 具体类型会在遇到另一种形态时让**整页**解析失败。
        #[derive(Deserialize)]
        struct Item {
            #[serde(rename = "f3")]
            pct: Option<serde_json::Value>,
            #[serde(rename = "f6")]
            amount: Option<serde_json::Value>,
            #[serde(rename = "f12")]
            code: Option<serde_json::Value>,
            #[serde(rename = "f14")]
            name: Option<serde_json::Value>,
            #[serde(rename = "f104")]
            up: Option<serde_json::Value>,
            #[serde(rename = "f105")]
            down: Option<serde_json::Value>,
            #[serde(rename = "f106")]
            flat: Option<serde_json::Value>,
            #[serde(rename = "f128")]
            leader: Option<serde_json::Value>,
            #[serde(rename = "f140")]
            leader_code: Option<serde_json::Value>,
            #[serde(rename = "f136")]
            leader_pct: Option<serde_json::Value>,
        }

        // pz 必须 ≤100：实测 200/500 会被静默截断成空页，与 datacenter 分页同一个坑
        const PAGE_SIZE: u32 = 100;
        const MAX_PAGES: u32 = 10;

        let path_of = |page: u32| {
            format!(
                "/api/qt/clist/get?pn={}&pz={}&po=1&np=1&fltt=2&invt=2&fid=f3&fs=m:90+t:2&\
fields=f3,f6,f12,f14,f104,f105,f106,f128,f140,f136",
                page, PAGE_SIZE
            )
        };

        // 首页单独抓：响应里的 `total` 告诉我们一共有多少页，后面才能一次并发补完。
        // 走 push2 多节点切换：单域名挂掉不影响整块功能。
        let first: Resp = serde_json::from_str(
            &crate::push2::get_text_any_node(&self.client, &path_of(1)).await?,
        )?;
        // 拆开用：total 决定还要抓几页，diff 是首页数据，两者都不再需要整体持有
        let Data { total, diff } = match first.data {
            Some(d) => d,
            None => return Ok(Vec::new()),
        };

        // 没有 total 就只认首页，不去盲猜后面还有没有
        let pages = match total {
            Some(t) if t > 0 => ((t + PAGE_SIZE - 1) / PAGE_SIZE).min(MAX_PAGES),
            _ => 1,
        };

        let mut pages_of_items: Vec<Vec<Item>> = vec![diff.unwrap_or_default()];
        if pages > 1 {
            let paths: Vec<String> = (2..=pages).map(path_of).collect();
            for body in fetch_pages_concurrent(paths, |path| {
                let client = &self.client;
                async move { crate::push2::get_text_any_node(client, &path).await }
            })
            .await?
            {
                let resp: Resp = serde_json::from_str(&body)?;
                pages_of_items.push(resp.data.and_then(|d| d.diff).unwrap_or_default());
            }
        }

        // 字段规整收在一处：东财同一字段在不同板块上可能是数字或字符串，
        // 以及「元 → 万元」的换算，翻页与并发都不该影响这里的口径。
        let convert = |it: Item| -> Option<BoardSnapshot> {
            let name = em_str(&it.name);
            if name.is_empty() {
                return None;
            }
            Some(BoardSnapshot {
                name,
                bk_code: em_str(&it.code),
                change_pct: em_num(&it.pct),
                // 接口给的是元，项目统一用万元
                amount: em_num(&it.amount) / 10_000.0,
                up_count: em_num(&it.up) as u32,
                down_count: em_num(&it.down) as u32,
                flat_count: em_num(&it.flat) as u32,
                leader: em_str_opt(&it.leader),
                leader_code: em_str_opt(&it.leader_code),
                leader_pct: em_num(&it.leader_pct),
            })
        };

        // 按页号顺序展开：顺序参与下游聚合，见 `PAGE_CONCURRENCY` 的说明。
        // 某一页不满说明已到末尾，即可收手；越界的页接口返回 `data: null`（实测），
        // 解析成空 vec 后自然被跳过，多请求几页没有副作用。
        let mut out: Vec<BoardSnapshot> = Vec::new();
        for items in pages_of_items {
            let short = items.len() < PAGE_SIZE as usize;
            out.extend(items.into_iter().filter_map(&convert));
            if short {
                break;
            }
        }

        Ok(out)
    }

    /// 东财快表覆盖的最新交易日
    ///
    /// 用作「能否走板块快照快路径」的守卫：快表只在收盘后落库，拿到的日期一定是
    /// 行情已定稿的交易日，与板块快照的口径天然一致。
    pub async fn fetch_latest_trade_date(&self) -> AppResult<Option<String>> {
        #[derive(Deserialize)]
        struct Resp {
            result: Option<ResultData>,
        }
        #[derive(Deserialize)]
        struct ResultData {
            data: Option<Vec<Item>>,
        }
        #[derive(Deserialize)]
        struct Item {
            #[serde(rename = "TRADE_DATE")]
            trade_date: Option<String>,
        }

        let text = self
            .datacenter_text(
                "http://datacenter.eastmoney.com/securities/api/data/v1/get?\
                 reportName=RPT_DMSK_TS_STOCKNEW&columns=TRADE_DATE&pageNumber=1&pageSize=1&\
                 source=WEB&client=WEB&sortColumns=SECUCODE&sortTypes=1",
            )
            .await?;
        let resp: Resp = serde_json::from_str(&text)?;
        Ok(resp
            .result
            .and_then(|r| r.data)
            .and_then(|d| d.into_iter().next())
            .and_then(|it| it.trade_date)
            .map(|s| s.split(' ').next().unwrap_or(&s).to_string()))
    }

    /// 全市场个股的最新交易日快照（涨跌幅 + 收盘价）
    ///
    /// 一次约 11 个请求就能拿到 5000+ 只个股，是计算市场宽度最快的通道。
    ///
    /// **口径注意**：该报表只覆盖沪深两市，**不含北交所**（实测 5196 只中北交所 0 只），
    /// 比个股行业归属表（约 5642 只）少几百只。与「逐股抓日线」的慢路径口径不同，
    /// 两者结果不可直接横向比较，报告里必须标出 `covered_count`。
    pub async fn fetch_market_snapshot(&self) -> AppResult<Vec<MarketBar>> {
        #[derive(Deserialize)]
        struct Resp {
            result: Option<ResultData>,
        }
        #[derive(Deserialize)]
        struct ResultData {
            pages: Option<u32>,
            data: Option<Vec<Item>>,
        }
        #[derive(Deserialize)]
        struct Item {
            #[serde(rename = "SECURITY_CODE", default)]
            code: String,
            #[serde(rename = "SECURITY_NAME_ABBR", default)]
            name: String,
            #[serde(rename = "CLOSE_PRICE", default)]
            close: Option<f64>,
            #[serde(rename = "CHANGE_RATE", default)]
            pct: Option<f64>,
        }

        // pageSize 服务端硬顶 500（实测传 1000/2000/5000 都只返回 500 行），分页绕不开
        const PAGE_SIZE: u32 = 500;
        // 防御性上限：40 页 × 500 = 20000 只，远超 A 股总量。串行翻页时靠「页不满就停」
        // 兜底，改成一次并发补页后必须先算出发多少请求，没有这个上限就会被上游的
        // 异常 pages 值放大成上万次请求。
        const MAX_PAGES: u32 = 40;

        let url_of = |page: u32| {
            format!(
                "http://datacenter.eastmoney.com/securities/api/data/v1/get?\
                 reportName=RPT_DMSK_TS_STOCKNEW&\
                 columns=SECURITY_CODE,SECURITY_NAME_ABBR,CLOSE_PRICE,CHANGE_RATE&\
                 pageNumber={}&pageSize={}&source=WEB&client=WEB&\
                 sortColumns=SECUCODE&sortTypes=1",
                page, PAGE_SIZE
            )
        };

        // 首页单独抓：它带回总页数，是后面一次并发补完的前提
        let first: Resp = serde_json::from_str(&self.datacenter_text(&url_of(1)).await?)?;
        let (pages, first_items) = match first.result {
            Some(r) => (r.pages.unwrap_or(1).min(MAX_PAGES), r.data.unwrap_or_default()),
            None => return Ok(Vec::new()),
        };

        let mut pages_of_items: Vec<Vec<Item>> = vec![first_items];
        if pages > 1 {
            let urls: Vec<String> = (2..=pages).map(url_of).collect();
            for body in fetch_pages_concurrent(urls, |url| {
                let this = self;
                async move { this.datacenter_text(&url).await }
            })
            .await?
            {
                let resp: Resp = serde_json::from_str(&body)?;
                pages_of_items.push(resp.result.and_then(|r| r.data).unwrap_or_default());
            }
        }

        let mut out: Vec<MarketBar> = Vec::new();
        for items in pages_of_items {
            for it in items {
                if it.code.is_empty() {
                    continue;
                }
                // 涨跌幅缺失的（停牌等）无法参与宽度统计，直接跳过而不是记 0 充数
                if let Some(pct) = it.pct {
                    out.push(MarketBar {
                        code: it.code,
                        name: it.name,
                        close: it.close.unwrap_or(0.0),
                        pct_change: pct,
                    });
                }
            }
        }

        Ok(out)
    }

    /// 两市成交额合计（万元）
    ///
    /// 用上证综指 + 深证综指两只指数的成交额相加，即媒体口径的「两市成交额」。
    /// 深证综指（399106）覆盖全部深市个股，不能用深证成指（399001，只有 500 只）替代。
    /// 指数报价里成交额是 `f48`（`f6` 在指数上为空），单位为元。
    pub async fn fetch_market_amount(&self) -> Option<f64> {
        const TARGETS: [&str; 2] = ["1.000001", "0.399106"];
        let mut total = 0.0_f64;
        let mut hit = 0usize;

        for secid in TARGETS {
            let url = format!(
                "http://push2delay.eastmoney.com/api/qt/stock/get?secid={}&fields=f48,f57&fltt=2&invt=2",
                secid
            );
            let text = match self.client.get(&url).send().await {
                Ok(r) => r.text().await.unwrap_or_default(),
                Err(_) => continue,
            };
            #[derive(Deserialize)]
            struct Resp {
                data: Option<Data>,
            }
            #[derive(Deserialize)]
            struct Data {
                #[serde(rename = "f48")]
                amount: Option<f64>,
            }
            if let Some(a) = serde_json::from_str::<Resp>(&text)
                .ok()
                .and_then(|r| r.data)
                .and_then(|d| d.amount)
            {
                if a > 0.0 {
                    total += a;
                    hit += 1;
                }
            }
        }

        // 只拿到一个市场时宁可返回 None：半边市场的数字会被当成全市场，比缺数更糟
        if hit == TARGETS.len() {
            Some(total / 10_000.0)
        } else {
            None
        }
    }

    /// 获取三大指数在指定交易日的收盘与涨跌幅
    ///
    /// 指数代码不能走 `tencent_secid` 推断（"000001" 会被判成深市个股 sz000001，
    /// 实际是平安银行），三个指数必须显式指定市场前缀：
    /// 上证指数 sh000001、深证成指 sz399001、创业板指 sz399006。
    pub async fn fetch_index_daily(&self, date: &str) -> AppResult<Vec<IndexDailyQuote>> {
        const TARGETS: [(&str, &str, &str); 3] = [
            ("sh000001", "000001", "上证指数"),
            ("sz399001", "399001", "深证成指"),
            ("sz399006", "399006", "创业板指"),
        ];

        let limit = Self::bars_needed_for(date);
        let mut out = Vec::new();

        for (secid, code, name) in TARGETS {
            let url = Self::tencent_kline_url(secid, "day", limit);
            let text = match self.client.get(&url).send().await {
                Ok(r) => r.text().await.unwrap_or_default(),
                Err(e) => {
                    tracing::warn!(index = name, error = %e, "指数行情抓取失败");
                    continue;
                }
            };
            let klines = match Self::parse_tencent_kline(&text, code, secid, "day") {
                Ok(k) => k,
                Err(e) => {
                    tracing::warn!(index = name, error = %e, "指数行情解析失败");
                    continue;
                }
            };
            if let Some(k) = klines.into_iter().find(|k| k.date == date) {
                out.push(IndexDailyQuote {
                    code: code.to_string(),
                    name: name.to_string(),
                    close: k.close,
                    pct_change: k.pct_change.unwrap_or(0.0),
                });
            }
        }

        Ok(out)
    }

    /// 取回某交易日需要向前回溯多少根日线
    ///
    /// 腾讯 K 线只支持「最近 N 根」，不支持按日期区间查询，因此要按目标日期
    /// 与今天的自然日距离估算交易日数量，再留出缓冲，避免目标日落在窗口之外。
    pub fn bars_needed_for(date: &str) -> u32 {
        let today = crate::clock::today();
        let target = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok();

        let days = match target {
            Some(t) => (today - t).num_days().max(0) as u32,
            // 日期无法解析时按最大窗口处理
            None => 800,
        };

        // 自然日 → 交易日（约 5/7），再加 15 根缓冲覆盖节假日与停牌
        let estimate = (days as f64 * 5.0 / 7.0).ceil() as u32 + 15;
        estimate.clamp(10, 800)
    }

    /// 行情源市场前缀: 6/900→sh, 920/4/8→bj, 其余→sz
    ///
    /// **920 必须是 bj**：北交所自 2024 年起启用 920xxx 新代码段，实测这批代码
    /// 挂 `sz` 前缀时腾讯与新浪都返回空（2026-08-31 一次全量抓取因此整体丢失
    /// 343 只，占 6.1%），挂 `bj` 才正常返回。老代码段 43/83/87/88 同样是 bj。
    ///
    /// **900 是沪市 B 股**，不能跟着 9 一起划走。腾讯对 B 股的两个前缀都能解析，
    /// 但返回的复权价略有差异，仍按真实归属给 sh。
    ///
    /// 这个函数同时被腾讯主源和新浪兜底源复用，两家的前缀规则一致。
    fn tencent_secid(code: &str) -> String {
        let prefix = if code.starts_with('6') || code.starts_with("900") {
            "sh"
        } else if code.starts_with("920") || code.starts_with('4') || code.starts_with('8') {
            "bj"
        } else {
            "sz"
        };
        format!("{}{}", prefix, code)
    }

    /// 抓一次腾讯 K 线，只区分「源级故障」与「其余一切」
    ///
    /// 注意**不**在这里过滤空结果：空结果是个股级缺失，对熔断而言等同于成功，
    /// 必须原样返回给调用方去走兜底源。
    async fn fetch_kline_tencent(
        &self,
        code: &str,
        secid: &str,
        period_key: &str,
        lmt: u32,
    ) -> TencentOutcome {
        let text = match self
            .client
            .get(Self::tencent_kline_url(secid, period_key, lmt))
            .send()
            .await
        {
            Ok(r) => match r.text().await {
                Ok(t) => t,
                Err(_) => return TencentOutcome::SourceDown,
            },
            Err(_) => return TencentOutcome::SourceDown,
        };
        match Self::parse_tencent_kline(&text, code, secid, period_key) {
            Ok(k) => TencentOutcome::Ok(k),
            // 走到这里基本只有一种可能：WAF 把 HTML 拦截页当 200 返回了
            Err(_) => TencentOutcome::SourceDown,
        }
    }

    /// 解析腾讯 K 线响应: data.<secid>.qfq<period> 或 <period>
    /// 每行格式 [日期, 开盘, 收盘, 最高, 最低, 成交量]，腾讯不含成交额/换手率，置 0/None
    fn parse_tencent_kline(
        raw: &str,
        code: &str,
        secid: &str,
        period_key: &str,
    ) -> AppResult<Vec<Kline>> {
        #[derive(Deserialize)]
        struct TencentResp {
            code: i32,
            data: Option<HashMap<String, TencentNode>>,
        }
        #[derive(Clone, Deserialize)]
        struct TencentNode {
            #[serde(rename = "qfqday")]
            qfq_day: Option<Vec<Vec<serde_json::Value>>>,
            #[serde(rename = "day")]
            day: Option<Vec<Vec<serde_json::Value>>>,
            #[serde(rename = "qfqweek")]
            qfq_week: Option<Vec<Vec<serde_json::Value>>>,
            #[serde(rename = "week")]
            week: Option<Vec<Vec<serde_json::Value>>>,
            #[serde(rename = "qfqmonth")]
            qfq_month: Option<Vec<Vec<serde_json::Value>>>,
            #[serde(rename = "month")]
            month: Option<Vec<Vec<serde_json::Value>>>,
        }

        let resp: TencentResp = serde_json::from_str(raw)?;
        if resp.code != 0 {
            return Ok(Vec::new());
        }
        let node = match resp.data.and_then(|m| m.get(secid).cloned()) {
            Some(node) => node,
            None => return Ok(Vec::new()),
        };

        let rows = match period_key {
            "week" => node.qfq_week.or(node.week),
            "month" => node.qfq_month.or(node.month),
            _ => node.qfq_day.or(node.day),
        };
        let rows = match rows {
            Some(rows) => rows,
            None => return Ok(Vec::new()),
        };

        let mut prev_close: Option<f64> = None;
        let mut klines = Vec::with_capacity(rows.len());
        for row in rows {
            if row.len() < 6 {
                continue;
            }
            let val = |i: usize| {
                row.get(i)
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.trim().parse::<f64>().ok())
            };
            let (date, open, close, high, low, volume) =
                match (row[0].as_str(), val(1), val(2), val(3), val(4), val(5)) {
                    (Some(date), Some(open), Some(close), Some(high), Some(low), Some(volume)) => {
                        (date.to_string(), open, close, high, low, volume)
                    }
                    _ => continue,
                };
            let pct_change = prev_close.map(|pc| {
                if pc != 0.0 {
                    (close - pc) / pc * 100.0
                } else {
                    0.0
                }
            });
            prev_close = Some(close);
            klines.push(Kline {
                stock_code: code.to_string(),
                date,
                open,
                close,
                high,
                low,
                volume,
                amount: 0.0,
                pct_change,
                turnover: None,
            });
        }
        Ok(klines)
    }

    /// 腾讯分钟时间 "202608051025" → "2026-08-05 10:25"
    fn format_minute_datetime(s: &str) -> String {
        if s.len() == 12 && s.bytes().all(|b| b.is_ascii_digit()) {
            format!(
                "{}-{}-{} {}:{}",
                &s[0..4],
                &s[4..6],
                &s[6..8],
                &s[8..10],
                &s[10..12]
            )
        } else {
            s.to_string()
        }
    }

    /// 解析腾讯分钟 K 线响应: data.<secid>.<m_period> = [[时间,开,收,高,低,量,{},额], ...]
    /// 时间格式 "202608051025"(YYYYMMDDHHMM)，第 7 列是空对象需跳过，第 8 列为成交额。
    fn parse_tencent_minute_kline(
        raw: &str,
        code: &str,
        secid: &str,
        minute_key: &str,
    ) -> AppResult<Vec<Kline>> {
        #[derive(Deserialize)]
        struct MinuteResp {
            code: i32,
            data: Option<HashMap<String, MinuteNode>>,
        }
        #[derive(Clone, Deserialize)]
        struct MinuteNode {
            #[serde(rename = "m1")]
            m1: Option<Vec<Vec<serde_json::Value>>>,
            #[serde(rename = "m5")]
            m5: Option<Vec<Vec<serde_json::Value>>>,
            #[serde(rename = "m15")]
            m15: Option<Vec<Vec<serde_json::Value>>>,
            #[serde(rename = "m30")]
            m30: Option<Vec<Vec<serde_json::Value>>>,
            #[serde(rename = "m60")]
            m60: Option<Vec<Vec<serde_json::Value>>>,
        }

        let resp: MinuteResp = serde_json::from_str(raw)?;
        if resp.code != 0 {
            return Ok(Vec::new());
        }
        let node = match resp.data.and_then(|m| m.get(secid).cloned()) {
            Some(node) => node,
            None => return Ok(Vec::new()),
        };

        let rows = match minute_key {
            "m1" => node.m1,
            "m15" => node.m15,
            "m30" => node.m30,
            "m60" => node.m60,
            _ => node.m5,
        };
        let rows = match rows {
            Some(rows) => rows,
            None => return Ok(Vec::new()),
        };

        let mut prev_close: Option<f64> = None;
        let mut klines = Vec::with_capacity(rows.len());
        for row in rows {
            if row.len() < 8 {
                continue;
            }
            let val = |i: usize| {
                row.get(i).and_then(|v| {
                    v.as_str()
                        .and_then(|s| s.trim().parse::<f64>().ok())
                        .or_else(|| v.as_f64())
                })
            };
            let (time, open, close, high, low, volume) =
                match (row[0].as_str(), val(1), val(2), val(3), val(4), val(5)) {
                    (Some(time), Some(open), Some(close), Some(high), Some(low), Some(volume)) => {
                        (time.to_string(), open, close, high, low, volume)
                    }
                    _ => continue,
                };
            let pct_change = prev_close.map(|pc| {
                if pc != 0.0 {
                    (close - pc) / pc * 100.0
                } else {
                    0.0
                }
            });
            prev_close = Some(close);
            klines.push(Kline {
                stock_code: code.to_string(),
                date: Self::format_minute_datetime(&time),
                open,
                close,
                high,
                low,
                volume,
                amount: val(7).unwrap_or(0.0),
                pct_change,
                turnover: None,
            });
        }
        Ok(klines)
    }

    /// 腾讯日/周/月 K 线 URL
    ///
    /// **必须用 https**：实测 `http://` 现在会被 302 跳到 https，多一次往返不说，
    /// 在接口被限流时 http 侧会直接返回 WAF 页面而非跳转，导致整条链路静默变空。
    fn tencent_kline_url(secid: &str, period_key: &str, limit: u32) -> String {
        format!(
            "https://web.ifzq.gtimg.cn/appstock/app/fqkline/get?param={},{},,,{},qfq",
            secid, period_key, limit
        )
    }

    /// 新浪日线 URL（腾讯不可用时的兜底源）
    ///
    /// 新浪与腾讯是互相独立的两个行情域，腾讯被 WAF 拦截时新浪通常仍然可用。
    /// 只支持日/周/月线（`scale` 分别对应 240/1200/7200），且返回的是**不复权**数据。
    fn sina_kline_url(symbol: &str, scale: u32, limit: u32) -> String {
        format!(
            "https://money.finance.sina.com.cn/quotes_service/api/json_v2.php/\
             CN_MarketData.getKLineData?symbol={}&scale={}&ma=no&datalen={}",
            symbol, scale, limit
        )
    }

    /// 解析新浪 K 线: [{day, open, high, low, close, volume}, ...]
    ///
    /// 与腾讯有两处差异，处理时必须注意：
    /// 1. 字段顺序是 `开/高/低/收`，腾讯是 `开/收/高/低`——照搬腾讯的解析会张冠李戴
    /// 2. 成交量单位是**股**，腾讯是**手**，这里统一换算成手，避免两源混用时数值差 100 倍
    ///
    /// 新浪不返回涨跌幅，用「上一根收盘价」现算，因此首根 K 线的 `pct_change` 为 None。
    fn parse_sina_kline(raw: &str, code: &str) -> AppResult<Vec<Kline>> {
        #[derive(Deserialize)]
        struct Row {
            day: String,
            open: String,
            high: String,
            low: String,
            close: String,
            volume: String,
        }

        let rows: Vec<Row> = serde_json::from_str(raw)?;
        let mut prev_close: Option<f64> = None;
        let mut klines = Vec::with_capacity(rows.len());

        for r in rows {
            let (open, high, low, close, volume) = match (
                r.open.parse::<f64>(),
                r.high.parse::<f64>(),
                r.low.parse::<f64>(),
                r.close.parse::<f64>(),
                r.volume.parse::<f64>(),
            ) {
                (Ok(o), Ok(h), Ok(l), Ok(c), Ok(v)) => (o, h, l, c, v),
                _ => continue,
            };
            let pct_change = prev_close.map(|pc| {
                if pc != 0.0 {
                    (close - pc) / pc * 100.0
                } else {
                    0.0
                }
            });
            prev_close = Some(close);
            klines.push(Kline {
                stock_code: code.to_string(),
                date: r.day,
                open,
                close,
                high,
                low,
                // 股 → 手
                volume: volume / 100.0,
                amount: 0.0,
                pct_change,
                turnover: None,
            });
        }
        Ok(klines)
    }

    /// 走新浪取日/周/月线（腾讯兜底路径）
    ///
    /// 返回 `(K线, 状态)`：新浪是本项目唯一能区分「无此标的」与「请求失败」的源——
    /// 它对不认识的 symbol 返回**字面量 `null`**（合法 JSON），而网络错误、WAF 页面、
    /// 限流响应都不会是它。实测 10/10 只未上市新股（301688、920071、688801…）均返回
    /// `null`，而正常标的一律返回数组。没有这个信号，日报只能把「未上市」当成
    /// 「抓取失败」报给用户。
    async fn fetch_kline_sina(
        &self,
        code: &str,
        period_key: &str,
        limit: u32,
    ) -> AppResult<(Vec<Kline>, KlineStatus)> {
        let symbol = Self::tencent_secid(code);
        let scale = match period_key {
            "week" => 1200,
            "month" => 7200,
            _ => 240,
        };
        let url = Self::sina_kline_url(&symbol, scale, limit.min(800));
        tracing::debug!("Fetching kline (sina fallback): {}", url);
        let text = self.client.get(&url).send().await?.text().await?;

        if text.trim() == "null" {
            return Ok((Vec::new(), KlineStatus::NoData));
        }
        match Self::parse_sina_kline(&text, code) {
            Ok(k) if !k.is_empty() => Ok((k, KlineStatus::Data)),
            // 解析成功但没有 K 线：标的存在但从未交易过
            Ok(_) => Ok((Vec::new(), KlineStatus::NoData)),
            Err(e) => Err(e),
        }
    }

    /// 腾讯分钟 K 线 URL（HTTPS, mkline 接口）
    fn tencent_minute_url(secid: &str, minute_key: &str, limit: u32) -> String {
        format!(
            "https://ifzq.gtimg.cn/appstock/app/kline/mkline?param={},{},,{}",
            secid, minute_key, limit
        )
    }
}

impl Default for EastmoneyFetcher {
    fn default() -> Self {
        Self::new()
    }
}

// --- 响应结构体 ---

/// 东财接口通用响应外壳
///
/// 只留被调用方用到的 `data`。东财还会回 `rc`（0 为成功）与 `rt`，目前未做
/// API 级错误码校验——接口异常靠 `data` 为 null 隐式暴露。若哪天要显式校验，
/// 把 `rc: Option<i32>` 加回来即可。
#[derive(Deserialize)]
struct EmResponse<T> {
    data: Option<T>,
}

#[derive(Deserialize)]
struct EmQuoteData {
    #[serde(rename = "f57")]
    code: String,
    #[serde(rename = "f58")]
    name: String,
    #[serde(rename = "f43", default)]
    price: f64,
    #[serde(rename = "f170", default)]
    pct_change: f64,
    #[serde(rename = "f169", default)]
    change: f64,
    #[serde(rename = "f46", default)]
    open: f64,
    #[serde(rename = "f60", default)]
    pre_close: f64,
    #[serde(rename = "f44", default)]
    high: f64,
    #[serde(rename = "f45", default)]
    low: f64,
    #[serde(rename = "f47", default)]
    volume: f64,
    #[serde(rename = "f48", default)]
    amount: f64,
    #[serde(rename = "f71", default)]
    turnover: f64,
}

impl EmQuoteData {
    fn to_quote(&self) -> Quote {
        Quote {
            code: self.code.clone(),
            name: self.name.clone(),
            price: self.price,
            pct_change: self.pct_change,
            change: self.change,
            open: self.open,
            pre_close: self.pre_close,
            high: self.high,
            low: self.low,
            volume: self.volume,
            amount: self.amount,
            turnover: if self.turnover > 0.0 { Some(self.turnover) } else { None },
            bid_price: None,
            ask_price: None,
            timestamp: EastmoneyFetcher::now_ts(),
        }
    }
}

#[derive(Deserialize)]
struct EmListItem {
    #[serde(rename = "f12", default)]
    code: String,
    #[serde(rename = "f14", default)]
    name: String,
    #[serde(rename = "f3", default)]
    change_pct: f64,
    #[serde(rename = "f128", default)]
    leader_stock: String,
    /// 领涨股**代码**（东财 f140）
    ///
    /// f128 是名称，早年被直接当代码用去查财报和匹配龙虎榜，两个功能因此
    /// 恒为空。这里必须单独存代码，不能被名称顶替。
    #[serde(rename = "f140", default)]
    leader_code: String,
    #[serde(rename = "f136", default)]
    leader_pct: f64,
    #[serde(rename = "f62", default)]
    net_amount: f64,
    #[serde(rename = "f104", default)]
    up_count: u32,
    #[serde(rename = "f105", default)]
    down_count: u32,
}

#[derive(Deserialize)]
struct EmListResponse {
    data: Option<EmListData>,
}

#[derive(Deserialize)]
struct EmListData {
    diff: Option<Vec<EmListItem>>,
}

#[async_trait]
impl DataFetcher for EastmoneyFetcher {
    async fn fetch_quote(&self, code: &str) -> AppResult<Quote> {
        let secid = Self::secid(code);
        let url = format!(
            "{}/get?secid={}&fltt=2&fields=f43,f44,f45,f46,f47,f48,f57,f58,f60,f71,f169,f170",
            EM_BASE, secid
        );

        tracing::debug!("Fetching quote: {}", url);

        let resp: EmResponse<EmQuoteData> = self
            .client
            .get(&url)
            .send()
            .await?
            .json()
            .await?;

        let data = resp.data.ok_or_else(|| {
            AppError::NotFound(format!("Quote not found for code: {}", code))
        })?;

        Ok(data.to_quote())
    }

    async fn fetch_quotes(&self, codes: &[&str]) -> AppResult<Vec<Quote>> {
        let mut quotes = Vec::with_capacity(codes.len());
        // 东方财富支持批量, 但为简单起见逐个获取
        for code in codes {
            match self.fetch_quote(code).await {
                Ok(q) => quotes.push(q),
                Err(e) => tracing::warn!("Failed to fetch quote for {}: {}", code, e),
            }
        }
        Ok(quotes)
    }

    async fn fetch_kline(
        &self,
        code: &str,
        period: &KlinePeriod,
        _start: Option<&str>,
        _end: Option<&str>,
        limit: Option<u32>,
    ) -> AppResult<Vec<Kline>> {
        // 取数逻辑（主源腾讯 https / 兜底新浪）在 `fetch_kline_with_status`，
        // 这里只需要 K 线、不关心状态，委托过去避免两处逻辑各自演化。
        self.fetch_kline_with_status(code, period, None, None, limit)
            .await
            .map(|(k, _)| k)
    }

    /// 腾讯日线主源是否已熔断（详见 trait 上的文档）
    fn primary_source_tripped(&self) -> bool {
        self.tencent_breaker.is_open()
    }

    /// 取日线并区分「无此标的 / 当日停牌 / 请求失败」
    ///
    /// 三种情况在返回值上都表现为空，但含义完全不同：前两种是市场本来就没有数据，
    /// 报成「抓取失败」会让用户误以为日报不完整。取数逻辑只此一份，
    /// `fetch_kline` 委托过来，避免两处各自演化。
    async fn fetch_kline_with_status(
        &self,
        code: &str,
        period: &KlinePeriod,
        _start: Option<&str>,
        end: Option<&str>,
        limit: Option<u32>,
    ) -> AppResult<(Vec<Kline>, KlineStatus)> {
        // 主源腾讯 `web.ifzq.gtimg.cn`（https，支持前复权）。东财的历史 K 线节点
        // push2his 在本机网络不可达、push2delay 不提供 K 线，二者都不能用作日线源。
        let secid = Self::tencent_secid(code);
        let lmt = limit.unwrap_or(365).min(800);

        let (period_key, is_minute) = match period {
            KlinePeriod::Daily => ("day", false),
            KlinePeriod::Weekly => ("week", false),
            KlinePeriod::Monthly => ("month", false),
            KlinePeriod::Minute1 => ("m1", true),
            KlinePeriod::Minute5 => ("m5", true),
            KlinePeriod::Minute15 => ("m15", true),
            KlinePeriod::Minute30 => ("m30", true),
            KlinePeriod::Minute60 => ("m60", true),
        };

        if is_minute {
            // 分钟线只有腾讯一个源，拿不到即无数据，不参与 Error 判定
            let url = Self::tencent_minute_url(&secid, period_key, lmt);
            let resp_text = self.client.get(&url).send().await?.text().await?;
            return match Self::parse_tencent_minute_kline(&resp_text, code, &secid, period_key) {
                Ok(k) if !k.is_empty() => Ok((k, KlineStatus::Data)),
                Ok(_) => Ok((Vec::new(), KlineStatus::NoData)),
                Err(e) => Err(e),
            };
        }

        // 主源失败有两种形态：网络层报错，或「成功返回但解析为空」（WAF 返回了 HTML 页面）。
        // 两种都值得再试一次新浪——两个域互不影响，这是日报不至于彻底跑不出来的关键兜底。
        //
        // 但 WAF 拦截是**持续**的：实测 6 分钟以上不恢复，期间每只股票仍要白跑一次
        // 腾讯（5642 只 × 约 50ms ≈ 15.6s，占冷启动 34.8s 的 45%），还把请求量翻倍
        // 更容易把兜底的新浪也拖进限流。所以主源挂了要能被记住，而不是每只股票
        // 重新试一次——这就是 `tencent_breaker`。
        //
        // 分钟线不参与熔断：它只有腾讯一个源，熔断与否都拿不到数据，让它记账只会
        // 污染日线主源的判定。
        let tencent: Option<Vec<Kline>> = if self.tencent_breaker.is_open() {
            None
        } else {
            match self
                .fetch_kline_tencent(code, &secid, period_key, lmt)
                .await
            {
                // 空结果也算：源能正常应答就是活着的证据
                TencentOutcome::Ok(k) => {
                    self.tencent_breaker.record_success();
                    Some(k).filter(|k| !k.is_empty())
                }
                TencentOutcome::SourceDown => {
                    self.tencent_breaker.record_failure();
                    None
                }
            }
        };

        // 「主源拿到了 K 线」不等于「拿到了要的那一天」。腾讯对部分标的返回的历史
        // 窗口偏短：2026-07-24 需要 43 根时，北交所 920 段 343 只 **100% 覆盖不到**，
        // 而兜底的新浪同参数下全部含该日。若照单全收，调用方只能把「主源窗口不够」
        // 解释成「当日停牌」，凭空多出几百只假停牌。
        // 所以主源必须覆盖到 end 才算拿到，否则继续问兜底源。end 为 None 时不做
        // 覆盖判定——所有历史调用点都传 None，行为与改动前完全一致。
        let covers_end = |k: &[Kline]| match end {
            Some(d) => k.iter().any(|x| x.date == d),
            None => true,
        };

        match tencent {
            Some(k) if covers_end(&k) => Ok((k, KlineStatus::Data)),
            other => match self.fetch_kline_sina(code, period_key, lmt).await {
                Ok(v) => Ok(v),
                // 兜底也没给出结论时，主源那份（可能只是窗口不够）仍是真实行情，
                // 交还调用方按日期判断，比报成「抓取失败」更接近事实。
                Err(_) => Ok((other.unwrap_or_default(), KlineStatus::Error)),
            },
        }
    }

    async fn fetch_sector_ranks(&self, sector_type: &str) -> AppResult<Vec<SectorRank>> {
        // 东财整段不可达时直接走新浪备选源，省一轮必然失败的请求
        if !crate::push2::is_quiet() {
            match self.fetch_sector_ranks_em_inner(sector_type).await {
                Ok(v) if !v.is_empty() => return Ok(v),
                Ok(_) => tracing::debug!(%sector_type, "东财板块涨幅榜空表，改用备选源"),
                Err(e) => tracing::debug!(%sector_type, error = %e, "东财板块涨幅榜失败，改用备选源"),
            }
        }
        self.fetch_sector_ranks_sina(sector_type).await
    }

    async fn fetch_sector_stocks(&self, sector_code: &str, page: u32, page_size: u32) -> AppResult<(Vec<SectorStock>, u32)> {
        // 板块成分股: fs=b:BKxxxx
        let fs = format!("b:{}", sector_code);
        let pn = page.max(1);
        let pz = page_size.max(1).min(100);

        let path = format!(
            "/api/qt/clist/get?pn={}&pz={}&po=1&np=1&fltt=2&invt=2&fid=f3&fs={}\
&fields=f12,f14,f2,f3,f4,f5,f6,f7,f15,f16,f17,f18",
            pn, pz, fs
        );

        tracing::debug!(page = pn, size = pz, "Fetching sector stocks via push2 nodes");

        #[derive(Deserialize)]
        struct EmStockListResponse {
            data: Option<EmStockListData>,
        }
        #[derive(Deserialize)]
        struct EmStockListData {
            diff: Option<Vec<EmStockItem>>,
            total: Option<u32>,
        }
        #[derive(Deserialize)]
        struct EmStockItem {
            #[serde(rename = "f12", default)]
            code: String,
            #[serde(rename = "f14", default)]
            name: String,
            #[serde(rename = "f2", default)]
            price: f64,
            #[serde(rename = "f3", default)]
            pct_change: f64,
            #[serde(rename = "f4", default)]
            change: f64,
            #[serde(rename = "f5", default)]
            volume: f64,
            #[serde(rename = "f6", default)]
            amount: f64,
            #[serde(rename = "f7", default)]
            turnover: f64,
            #[serde(rename = "f15", default)]
            high: f64,
            #[serde(rename = "f16", default)]
            low: f64,
            #[serde(rename = "f17", default)]
            open: f64,
            #[serde(rename = "f18", default)]
            pre_close: f64,
        }

        let body = crate::push2::get_text_any_node(&self.client, &path).await?;
        let resp: EmStockListResponse = serde_json::from_str(&body)?;

        let data = resp.data.ok_or_else(|| {
            AppError::NotFound(format!("Sector stocks not found for: {}", sector_code))
        })?;

        let total = data.total.unwrap_or(0);
        let stocks = data.diff.unwrap_or_default().iter().map(|item| SectorStock {
            code: item.code.clone(),
            name: item.name.clone(),
            price: item.price,
            pct_change: item.pct_change,
            change: item.change,
            volume: item.volume,
            amount: item.amount,
            high: item.high,
            low: item.low,
            open: item.open,
            pre_close: item.pre_close,
            turnover: item.turnover,
        }).collect();

        Ok((stocks, total))
    }

    async fn fetch_market_overview(&self) -> AppResult<MarketOverview> {
        // 三大指数。东财 `stock/get` 在主源被风控时整块返回空，
        // 所以先走东财、拿到空表就整体换腾讯（腾讯同时给出成交额，能算总成交）。
        let mut indices = self.fetch_indices_em().await.unwrap_or_default();
        if indices.is_empty() {
            match self.fetch_indices_tencent().await {
                Ok(v) if !v.is_empty() => {
                    tracing::info!(count = v.len(), "指数走腾讯源");
                    indices = v;
                }
                Ok(_) => tracing::warn!("腾讯指数源也返回空"),
                Err(e) => tracing::warn!(error = %e, "腾讯指数源失败"),
            }
        }

        // 两市总成交额 = 上证 + 深证（创业板指已包含在深证内，不能再加）
        let total_amount = indices
            .iter()
            .filter(|i| i.code == "000001" || i.code == "399001")
            .map(|i| i.amount)
            .sum::<f64>();

        // 涨跌家数：走多节点，静默期会直接失败 → 保留 0 并在前端标注「—」
        let (up, down, flat) = match self.fetch_up_down_count().await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "涨跌家数获取失败");
                (0, 0, 0)
            }
        };

        Ok(MarketOverview {
            indices,
            up_count: up,
            down_count: down,
            flat_count: flat,
            limit_up_count: 0,
            limit_down_count: 0,
            total_amount,
            timestamp: Self::now_ts(),
        })
    }


    // ============ Phase 4: 资金流向 ============

    async fn fetch_capital_flow(&self, code: &str, limit: Option<u32>) -> AppResult<Vec<CapitalFlow>> {
        let secid = Self::secid(code);
        let lmt = limit.unwrap_or(60);
        // fflow/daykline/get with klt=101 for daily data
        // fields2: f51=日期,f52=主力净流入,f53=小单净流入,f54=中单净流入,
        //   f55=大单净流入,f56=超大单净流入,f57=主力净流入占比,f58=小单占比,
        //   f59=中单占比,f60=大单占比,f61=超大单占比,f62=收盘价,f63=涨跌幅
        let url = format!(
            "http://push2delay.eastmoney.com/api/qt/stock/fflow/daykline/get?lmt={}&klt=101&secid={}&fields1=f1,f2,f3,f7&fields2=f51,f52,f53,f54,f55,f56,f57,f58,f59,f60,f61,f62,f63",
            lmt, secid
        );

        tracing::debug!("Fetching capital flow: {}", url);

        #[derive(Deserialize)]
        struct FflowData {
            klines: Option<Vec<String>>,
        }

        let resp: EmResponse<FflowData> = self.client.get(&url).send().await?.json().await?;
        let klines = resp.data.and_then(|d| d.klines).unwrap_or_default();

        let flows: Vec<CapitalFlow> = klines.iter().filter_map(|line| {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() < 13 { return None; }
            // f51=日期, f52=主力净流入, f53=小单净流入, f54=中单净流入,
            // f55=大单净流入, f56=超大单净流入, f57=主力净流入占比,
            // f58=小单占比, f59=中单占比, f60=大单占比, f61=超大单占比,
            // f62=收盘价, f63=涨跌幅
            Some(CapitalFlow {
                stock_code: code.to_string(),
                date: parts[0].to_string(),
                main_net_inflow: parts[1].parse().ok()?,             // f52 主力净流入
                small_net_inflow: parts[2].parse().ok()?,            // f53 小单净流入
                medium_net_inflow: parts[3].parse().ok()?,           // f54 中单净流入
                large_net_inflow: parts[4].parse().ok()?,            // f55 大单净流入
                huge_net_inflow: parts[5].parse().ok()?,             // f56 超大单净流入
                main_net_ratio: parts[6].parse().ok()?,              // f57 主力净流入占比
                small_net_ratio: parts.get(7).and_then(|v| v.parse().ok()).unwrap_or(0.0),  // f58
                medium_net_ratio: parts.get(8).and_then(|v| v.parse().ok()).unwrap_or(0.0), // f59
                large_net_ratio: parts.get(9).and_then(|v| v.parse().ok()).unwrap_or(0.0),  // f60
                huge_net_ratio: parts.get(10).and_then(|v| v.parse().ok()).unwrap_or(0.0),  // f61
                close_price: parts.get(11).and_then(|v| v.parse().ok()).unwrap_or(0.0),     // f62
                pct_change: parts.get(12).and_then(|v| v.parse().ok()).unwrap_or(0.0),      // f63
            })
        }).collect();

        Ok(flows)
    }

    async fn fetch_sector_capital_flows(&self) -> AppResult<Vec<SectorCapitalFlow>> {
        // 东方财富行业板块资金流
        // f62=主力净流入, f184=主力净占比, f66=超大单净额, f69=超大单占比,
        // f72=大单净额, f75=大单占比, f78=中单净额, f81=中单占比,
        // f84=小单净额, f87=小单占比
        let path = "/api/qt/clist/get?pn=1&pz=50&po=1&np=1&fltt=2&invt=2&fid=f62&fs=m:90+t:2\
&fields=f12,f14,f3,f62,f184,f66,f69,f72,f75,f78,f81,f84,f87";

        // 集群静默期内直接返回空表：新浪备选源不提供板块资金分档数据，
        // 与其返回半真半假的数据，不如让上层明确显示「暂无数据」。
        if crate::push2::is_quiet() {
            tracing::debug!("东财集群不可达，板块资金流暂不可用（备选源无此数据）");
            return Ok(Vec::new());
        }

        tracing::debug!("Fetching sector capital flows via push2 nodes");

        /// 板块资金流条目
        ///
        /// 只声明调用方实际读取的字段，serde 默认忽略其余响应字段。
        /// 东财同时返回各档资金的**占比**（超大单 f69 / 大单 f75 / 中单 f81 /
        /// 小单 f87），目前界面只展示净额，故未声明——需要时按 f-code 加回即可。
        #[derive(Deserialize)]
        struct SectorFlowItem {
            #[serde(rename = "f12", default)]
            code: String,
            #[serde(rename = "f14", default)]
            name: String,
            #[serde(rename = "f3", default)]
            pct_change: f64,
            #[serde(rename = "f62", default)]
            main_net: f64,
            #[serde(rename = "f184", default)]
            main_ratio: f64,
            #[serde(rename = "f66", default)]
            huge_net: f64,
            #[serde(rename = "f72", default)]
            large_net: f64,
            #[serde(rename = "f78", default)]
            medium_net: f64,
            #[serde(rename = "f84", default)]
            small_net: f64,
        }

        #[derive(Deserialize)]
        struct SectorFlowListData {
            diff: Option<Vec<SectorFlowItem>>,
        }

        let body = match crate::push2::get_text_any_node(&self.client, path).await {
            Ok(b) => b,
            Err(e) => {
                // 备用源没有这份数据，降级成空表而不是 500，页面显示「暂无数据」
                tracing::debug!(error = %e, "板块资金流主源不可达，返回空表");
                return Ok(Vec::new());
            }
        };
        let resp: EmResponse<SectorFlowListData> = match serde_json::from_str(&body) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "板块资金流响应解析失败，返回空表");
                return Ok(Vec::new());
            }
        };
        let items = resp.data.and_then(|d| d.diff).unwrap_or_default();

        let flows: Vec<SectorCapitalFlow> = items.iter().map(|item| SectorCapitalFlow {
            code: item.code.clone(),
            name: item.name.clone(),
            main_net_inflow: item.main_net,
            main_net_ratio: item.main_ratio,
            huge_net_inflow: item.huge_net,
            large_net_inflow: item.large_net,
            medium_net_inflow: item.medium_net,
            small_net_inflow: item.small_net,
            pct_change: item.pct_change,
            stock_count: None,
        }).collect();

        Ok(flows)
    }

    async fn fetch_north_bound_flow(&self, limit: Option<u32>) -> AppResult<Vec<NorthBoundFlow>> {
        let lmt = limit.unwrap_or(30);
        // 北向资金 = 沪股通(klt=101) + 深股通(klt=102)
        // fields2: f51=日期, f52=当日净买入(万元), f53=每日额度余额(万元), f54=?, f55=?, f56=累计净买入(万元)
        let sh_url = format!(
            "http://push2delay.eastmoney.com/api/qt/kamt.kline/get?fields1=f1,f2,f3,f4&fields2=f51,f52,f53,f56&klt=101&lmt={}",
            lmt
        );
        let sz_url = format!(
            "http://push2delay.eastmoney.com/api/qt/kamt.kline/get?fields1=f1,f2,f3,f4&fields2=f51,f52,f53,f56&klt=102&lmt={}",
            lmt
        );

        #[derive(Deserialize, Default)]
        struct KamtData {
            #[serde(default)]
            hk2sh: Vec<String>,
            #[serde(default)]
            hk2sz: Vec<String>,
        }

        let (sh_resp, sz_resp) = tokio::join!(
            self.client.get(&sh_url).send(),
            self.client.get(&sz_url).send(),
        );

        let sh_data = match sh_resp {
            Ok(r) => r.json::<EmResponse<KamtData>>().await.ok()
                .and_then(|r| r.data).unwrap_or_default(),
            Err(_) => KamtData::default(),
        };
        let sz_data = match sz_resp {
            Ok(r) => r.json::<EmResponse<KamtData>>().await.ok()
                .and_then(|r| r.data).unwrap_or_default(),
            Err(_) => KamtData::default(),
        };

        // Merge SH + SZ by date
        use std::collections::BTreeMap;
        let mut date_map: BTreeMap<String, (f64, f64, f64, f64)> = BTreeMap::new();

        for line in &sh_data.hk2sh {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() < 3 { continue; }
            let date = parts[0].to_string();
            let net: f64 = parts[1].parse().unwrap_or(0.0);
            let balance: f64 = parts[2].parse().unwrap_or(0.0);
            let entry = date_map.entry(date).or_insert((0.0, 0.0, 0.0, 0.0));
            entry.0 = net;     // sh_net
            entry.2 = balance; // sh_balance
        }

        for line in &sz_data.hk2sz {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() < 3 { continue; }
            let date = parts[0].to_string();
            let net: f64 = parts[1].parse().unwrap_or(0.0);
            let balance: f64 = parts[2].parse().unwrap_or(0.0);
            let entry = date_map.entry(date).or_insert((0.0, 0.0, 0.0, 0.0));
            entry.1 = net;     // sz_net
            entry.3 = balance; // sz_balance
        }

        let flows: Vec<NorthBoundFlow> = date_map.into_iter()
            .map(|(date, (sh_net, sz_net, sh_balance, sz_balance))| {
                NorthBoundFlow {
                    date,
                    sh_net_inflow: sh_net,
                    sz_net_inflow: sz_net,
                    total_net_inflow: sh_net + sz_net,
                    sh_balance,
                    sz_balance,
                }
            })
            .collect();

        Ok(flows)
    }

    async fn fetch_north_bound_holdings(&self) -> AppResult<Vec<NorthBoundHoldings>> {
        // 自 2024-08-19 起交易所不再披露北向每日净流入，仅按季度披露持股汇总
        let url = "https://datacenter-web.eastmoney.com/api/data/v1/get?reportName=RPT_MUTUAL_MARKET_STA&columns=ALL&pageNumber=1&pageSize=40&sortTypes=-1&sortColumns=HOLD_DATE&source=WEB&client=WEB";

        tracing::debug!("Fetching north bound holdings: {}", url);

        #[derive(Deserialize)]
        struct HoldingsResp {
            result: Option<HoldingsResult>,
        }
        #[derive(Deserialize)]
        struct HoldingsResult {
            data: Option<Vec<HoldingsItem>>,
        }
        #[derive(Deserialize)]
        struct HoldingsItem {
            #[serde(rename = "HOLD_DATE")]
            hold_date: Option<String>,
            #[serde(rename = "HOLD_MARKET_CAP", default)]
            hold_market_cap: f64,
            #[serde(rename = "ADD_MARKET_CAP", default)]
            add_market_cap: f64,
            #[serde(rename = "CHANGE_RATE", default)]
            change_rate: f64,
        }

        let resp: HoldingsResp = self.client.get(url).send().await?.json().await?;
        let data = resp.result.and_then(|r| r.data).unwrap_or_default();

        let items: Vec<NorthBoundHoldings> = data.iter().map(|item| NorthBoundHoldings {
            date: item.hold_date.as_ref()
                .map(|d| if d.len() >= 10 { d[..10].to_string() } else { d.to_string() })
                .unwrap_or_default(),
            hold_market_cap: item.hold_market_cap,
            add_market_cap: item.add_market_cap,
            change_rate: item.change_rate,
        }).collect();

        Ok(items)
    }

    async fn fetch_market_capital_flow(&self) -> AppResult<MarketCapitalFlow> {
        // 沪深两市主力资金 = 上证指数(1.000001) + 深证成指(0.399001)
        // f62=主力净流入, f66=超大单, f72=大单, f78=中单, f84=小单（单位：元）
        let path = "/api/qt/ulist.np/get?fltt=2&secids=1.000001,0.399001\
&fields=f12,f14,f62,f66,f72,f78,f84";

        tracing::debug!("Fetching market capital flow via push2 nodes");

        #[derive(Deserialize)]
        struct FlowResp {
            data: Option<FlowData>,
        }
        #[derive(Deserialize)]
        struct FlowData {
            diff: Option<Vec<FlowItem>>,
        }
        #[derive(Deserialize)]
        struct FlowItem {
            #[serde(rename = "f62", default)]
            main_net_inflow: f64,
            #[serde(rename = "f66", default)]
            huge_net_inflow: f64,
            #[serde(rename = "f72", default)]
            large_net_inflow: f64,
            #[serde(rename = "f78", default)]
            medium_net_inflow: f64,
            #[serde(rename = "f84", default)]
            small_net_inflow: f64,
        }

        let body = crate::push2::get_text_any_node(&self.client, path).await?;
        let resp: FlowResp = serde_json::from_str(&body)?;
        let items = resp.data.and_then(|d| d.diff).unwrap_or_default();

        let mut total = MarketCapitalFlow {
            date: crate::clock::today_str(),
            main_net_inflow: 0.0,
            huge_net_inflow: 0.0,
            large_net_inflow: 0.0,
            medium_net_inflow: 0.0,
            small_net_inflow: 0.0,
        };
        for item in items {
            total.main_net_inflow += item.main_net_inflow;
            total.huge_net_inflow += item.huge_net_inflow;
            total.large_net_inflow += item.large_net_inflow;
            total.medium_net_inflow += item.medium_net_inflow;
            total.small_net_inflow += item.small_net_inflow;
        }

        Ok(total)
    }

    async fn fetch_margin_trading(&self, limit: Option<u32>) -> AppResult<Vec<MarginTrading>> {
        let lmt = limit.unwrap_or(30);
        let url = format!(
            "https://datacenter-web.eastmoney.com/api/data/v1/get?reportName=RPTA_RZRQ_LSHJ&columns=ALL&pageNumber=1&pageSize={}&sortTypes=-1&sortColumns=DIM_DATE&source=WEB&client=WEB",
            lmt
        );

        tracing::debug!("Fetching margin trading: {}", url);

        #[derive(Deserialize)]
        struct MarginResp {
            result: Option<MarginResult>,
        }
        #[derive(Deserialize)]
        struct MarginResult {
            data: Option<Vec<MarginItem>>,
        }
        #[derive(Deserialize)]
        struct MarginItem {
            #[serde(rename = "DIM_DATE")]
            dim_date: Option<String>,
            #[serde(rename = "RZYE", default)]
            margin_balance: f64,
            #[serde(rename = "RQYE", default)]
            short_balance: f64,
            #[serde(rename = "RZRQYE", default)]
            total_balance: f64,
            #[serde(rename = "RZMRE", default)]
            margin_buy: f64,
            #[serde(rename = "RZCHE", default)]
            margin_repay: f64,
            #[serde(rename = "RZJME", default)]
            margin_net_buy: f64,
            #[serde(rename = "RQMCL", default)]
            short_sell_vol: f64,
            #[serde(rename = "RQCHL", default)]
            short_repay_vol: f64,
            #[serde(rename = "RQJMG", default)]
            short_net_sell: f64,
        }

        let resp: MarginResp = self.client.get(&url).send().await?.json().await?;
        let data = resp.result.and_then(|r| r.data).unwrap_or_default();

        let items: Vec<MarginTrading> = data.iter().map(|item| MarginTrading {
            date: item.dim_date.as_ref()
                .map(|d| if d.len() >= 10 { d[..10].to_string() } else { d.to_string() })
                .unwrap_or_default(),
            margin_balance: item.margin_balance,
            short_balance: item.short_balance,
            total_balance: item.total_balance,
            margin_buy: item.margin_buy,
            margin_repay: item.margin_repay,
            margin_net_buy: item.margin_net_buy,
            short_sell_vol: item.short_sell_vol,
            short_repay_vol: item.short_repay_vol,
            short_net_sell: item.short_net_sell,
        }).collect();

        Ok(items)
    }

    // ============ Phase 4: 涨停复盘 ============

    async fn fetch_limit_ups(&self) -> AppResult<Vec<LimitUpStock>> {
        let path = "/api/qt/clist/get?pn=1&pz=200&po=1&np=1&fltt=2&invt=2&fid=f3\
&fs=m:0+t:6,m:0+t:80,m:1+t:2,m:1+t:23\
&fields=f2,f3,f4,f5,f6,f7,f8,f9,f10,f12,f14,f15,f16,f17,f18,f20,f21,f22,f23,f62,f100,f184";

        tracing::debug!("Fetching limit ups via push2 nodes");

        // 东财涨停池只有「涨跌幅」一个信号，连板天数恒 1、封单额恒 0，
        // 且 9.5% 阈值对创业板/科创(20%)、北交所(30%)、ST(5%) 全错。
        // 这里是本项目数据质量最差的一处，故**优先**用同花顺：
        // 它按真实板块规则判定，并直接给连板天数/封单额/涨停时间/涨停原因。
        if self.fuyao.is_configured() {
            match self.fuyao.fetch_limit_ups().await {
                Ok(v) if !v.is_empty() => {
                    tracing::info!(count = v.len(), "涨停池走同花顺源");
                    return Ok(v);
                }
                Ok(_) => tracing::warn!("同花顺涨停池返回空，回落东财"),
                Err(e) => tracing::warn!(error = %e, "同花顺涨停池失败，回落东财"),
            }
        }

        let body = crate::push2::get_text_any_node(&self.client, path).await?;
        let resp: EmListResponse = serde_json::from_str(&body)?;
        let items = resp.data.and_then(|d| d.diff).unwrap_or_default();

        // Filter limit-up stocks (pct_change >= ~9.5%)
        let limit_ups: Vec<LimitUpStock> = items.iter().filter_map(|item| {
            if item.change_pct < 9.5 { return None; }
            // Need more fields than EmListItem provides
            Some(LimitUpStock {
                code: item.code.clone(),
                name: item.name.clone(),
                price: 0.0,
                pct_change: item.change_pct,
                consecutive_days: 1,
                first_limit_time: None,
                last_limit_time: None,
                break_count: 0,
                seal_volume: 0.0,
                seal_amount: 0.0,
                seal_strength: None,
                amount: 0.0,
                turnover: 0.0,
                float_market_cap: 0.0,
                industry: None,
                reason: None,
                amplitude: 0.0,
            })
        }).collect();

        Ok(limit_ups)
    }

    async fn fetch_limit_downs(&self) -> AppResult<Vec<LimitDownStock>> {
        let path = "/api/qt/clist/get?pn=1&pz=200&po=1&np=1&fltt=2&invt=2&fid=f3\
&fs=m:0+t:6,m:0+t:80,m:1+t:2,m:1+t:23\
&fields=f2,f3,f4,f5,f6,f7,f8,f9,f10,f12,f14,f15,f16,f17,f18,f20,f21,f62,f100";

        tracing::debug!("Fetching limit downs via push2 nodes");

        if self.fuyao.is_configured() {
            match self.fuyao.fetch_limit_downs().await {
                Ok(v) if !v.is_empty() => {
                    tracing::info!(count = v.len(), "跌停池走同花顺源");
                    return Ok(v);
                }
                Ok(_) => tracing::warn!("同花顺跌停池返回空，回落东财"),
                Err(e) => tracing::warn!(error = %e, "同花顺跌停池失败，回落东财"),
            }
        }

        let body = crate::push2::get_text_any_node(&self.client, path).await?;
        let resp: EmListResponse = serde_json::from_str(&body)?;
        let items = resp.data.and_then(|d| d.diff).unwrap_or_default();

        let limit_downs: Vec<LimitDownStock> = items.iter().filter_map(|item| {
            if item.change_pct > -9.5 { return None; }
            Some(LimitDownStock {
                code: item.code.clone(),
                name: item.name.clone(),
                price: 0.0,
                pct_change: item.change_pct,
                consecutive_days: 1,
                seal_amount: 0.0,
                amount: 0.0,
                turnover: 0.0,
                industry: None,
            })
        }).collect();

        Ok(limit_downs)
    }

    async fn fetch_broken_boards(&self) -> AppResult<Vec<BrokenBoardStock>> {
        // 炸板股: 振幅较大且最高涨幅达到涨停但收盘未封板
        let path = "/api/qt/clist/get?pn=1&pz=200&po=1&np=1&fltt=2&invt=2&fid=f7\
&fs=m:0+t:6,m:0+t:80,m:1+t:2,m:1+t:23\
&fields=f2,f3,f4,f5,f6,f7,f8,f9,f10,f12,f14,f15,f16,f17,f18,f20,f21";

        tracing::debug!("Fetching broken boards via push2 nodes");

        // 东财没有炸板集合，只能用「涨幅 5~9.5%」冒充，把正常上涨的票也算进去。
        // 同花顺有真实炸板池（含开板次数、换手率），优先用它。
        if self.fuyao.is_configured() {
            match self.fuyao.fetch_broken_boards().await {
                Ok(v) if !v.is_empty() => {
                    tracing::info!(count = v.len(), "炸板池走同花顺源");
                    return Ok(v);
                }
                Ok(_) => tracing::warn!("同花顺炸板池返回空，回落东财"),
                Err(e) => tracing::warn!(error = %e, "同花顺炸板池失败，回落东财"),
            }
        }

        let body = crate::push2::get_text_any_node(&self.client, path).await?;
        let resp: EmListResponse = serde_json::from_str(&body)?;
        let items = resp.data.and_then(|d| d.diff).unwrap_or_default();

        // 炸板判断: 涨跌幅在5~9.5%之间且振幅大于8%
        let broken: Vec<BrokenBoardStock> = items.iter().filter_map(|item| {
            if item.change_pct >= 5.0 && item.change_pct < 9.5 {
                Some(BrokenBoardStock {
                    code: item.code.clone(),
                    name: item.name.clone(),
                    price: 0.0,
                    pct_change: item.change_pct,
                    max_pct: 0.0,
                    break_count: 1,
                    amount: 0.0,
                    turnover: 0.0,
                    industry: None,
                })
            } else {
                None
            }
        }).collect();

        Ok(broken)
    }

    async fn fetch_limit_up_review(&self) -> AppResult<LimitUpReview> {
        // 合并涨停/跌停/炸板数据
        let (limit_ups, limit_downs, broken_boards) = tokio::join!(
            self.fetch_limit_ups(),
            self.fetch_limit_downs(),
            self.fetch_broken_boards(),
        );

        let limit_ups = limit_ups.unwrap_or_default();
        let limit_downs = limit_downs.unwrap_or_default();
        let broken_boards = broken_boards.unwrap_or_default();

        // 统计涨停原因分布（从股票名称和行业推断）
        let mut reason_map: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        for stock in &limit_ups {
            if let Some(ref reason) = stock.reason {
                *reason_map.entry(reason.clone()).or_default() += 1;
            }
        }
        let reason_distribution: Vec<(String, u32)> = reason_map.into_iter().collect();

        // 连板梯队
        let mut tier_map: std::collections::HashMap<u32, Vec<LimitUpStock>> = std::collections::HashMap::new();
        let mut max_consecutive = 0u32;
        for stock in &limit_ups {
            let days = stock.consecutive_days;
            if days > max_consecutive { max_consecutive = days; }
            tier_map.entry(days).or_default().push(stock.clone());
        }

        let mut consecutive_tiers: Vec<ConsecutiveBoardTier> = tier_map.into_iter()
            .map(|(days, stocks)| ConsecutiveBoardTier {
                days,
                count: stocks.len() as u32,
                stocks,
            })
            .collect();
        consecutive_tiers.sort_by_key(|t| t.days);

        let one_word_count = limit_ups.iter().filter(|s| s.amplitude < 0.5).count() as u32;
        let t_shape_count = limit_ups.iter().filter(|s| s.amplitude > 0.5 && s.amplitude < 5.0 && s.first_limit_time.as_deref() != Some("09:25:00")).count() as u32;

        let total = (limit_ups.len() + broken_boards.len()) as f64;
        let break_rate = if total > 0.0 { (broken_boards.len() as f64) / total * 100.0 } else { 0.0 };

        Ok(LimitUpReview {
            limit_up_count: limit_ups.len() as u32,
            one_word_count,
            t_shape_count,
            broken_count: broken_boards.len() as u32,
            limit_down_count: limit_downs.len() as u32,
            max_consecutive,
            break_rate,
            limit_ups,
            limit_downs,
            broken_boards,
            consecutive_tiers,
            reason_distribution,
            timestamp: Self::now_ts(),
        })
    }

    // ============ Phase 4: 龙虎榜 ============

    async fn fetch_dragon_tiger(&self, date: Option<&str>) -> AppResult<Vec<DragonTigerEntry>> {
        // 报表名（注意：原 RPT_DAILY_BILLBOARDTRADING 已不存在，返回空）
        let report = "RPT_DAILYBILLBOARD_DETAILSNEW";

        // 前端可能传 YYYYMMDD，接口 TRADE_DATE 存储为 YYYY-MM-DD HH:MM:SS，统一成 YYYY-MM-DD
        let trade_date = date.unwrap_or("");
        let trade_date = if trade_date.len() == 8 && trade_date.chars().all(|c| c.is_ascii_digit()) {
            format!("{}-{}-{}", &trade_date[0..4], &trade_date[4..6], &trade_date[6..8])
        } else {
            trade_date.to_string()
        };
        let filter = if trade_date.is_empty() {
            String::new()
        } else {
            format!("&filter=(TRADE_DATE='{}')", trade_date)
        };

        let url = format!(
            "http://datacenter.eastmoney.com/securities/api/data/v1/get?reportName={}&columns=ALL&pageNumber=1&pageSize=100&sortTypes=-1&sortColumns=TRADE_DATE{}&source=WEB&client=WEB",
            report, filter
        );

        tracing::debug!("Fetching dragon tiger: {}", url);

        // 同花顺优先：东财 datacenter 这条报表链在前面的实测里反复 500/空，
        // 而同花顺龙虎榜带净买入、机构净买入、人气排名，字段更齐。
        if self.fuyao.is_configured() {
            let d = if trade_date.is_empty() { None } else { Some(trade_date.as_str()) };
            match self.fuyao.fetch_dragon_tiger(d).await {
                Ok(v) if !v.is_empty() => {
                    tracing::info!(count = v.len(), "龙虎榜走同花顺源");
                    return Ok(v);
                }
                Ok(_) => tracing::warn!("同花顺龙虎榜返回空，回落东财"),
                Err(e) => tracing::warn!(error = %e, "同花顺龙虎榜失败，回落东财"),
            }
        }

        #[derive(Deserialize)]
        struct DtResp {
            result: Option<DtResult>,
        }
        #[derive(Deserialize)]
        struct DtResult {
            data: Option<Vec<DtItem>>,
        }
        #[derive(Deserialize)]
        struct DtItem {
            #[serde(rename = "SECURITY_CODE")]
            security_code: Option<String>,
            #[serde(rename = "SECURITY_NAME_ABBR")]
            security_name: Option<String>,
            #[serde(rename = "TRADE_DATE")]
            trade_date: Option<String>,
            #[serde(rename = "CLOSE_PRICE", default)]
            close_price: Option<f64>,
            #[serde(rename = "CHANGE_RATE", default)]
            change_rate: Option<f64>,
            #[serde(rename = "TURNOVERRATE", default)]
            turnover_rate: Option<f64>,
            #[serde(rename = "ACCUM_AMOUNT", default)]
            accum_amount: Option<f64>,
            #[serde(rename = "BILLBOARD_NET_AMT", default)]
            net_amount: Option<f64>,
            #[serde(rename = "BILLBOARD_BUY_AMT", default)]
            total_buy: Option<f64>,
            #[serde(rename = "BILLBOARD_SELL_AMT", default)]
            total_sell: Option<f64>,
            #[serde(rename = "DEAL_AMOUNT_RATIO", default)]
            lhb_ratio: Option<f64>,
            #[serde(rename = "EXPLANATION")]
            reason: Option<String>,
        }

        // datacenter.eastmoney.com 会返回 Content-Encoding: gzip 的响应体（且非合法 gzip），
        // 直接 .json() 会报 "error decoding response body"。强制 identity 并在本地解析规避。
        let resp_text = self.client
            .get(&url)
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .send()
            .await?
            .text()
            .await?;
        let resp: DtResp = serde_json::from_str(&resp_text)?;
        let data = resp.result.and_then(|r| r.data).unwrap_or_default();

        // 接口金额单位为「元」，模型与前端均按「万元」处理，统一除以 10000
        let to_wan = |v: Option<f64>| v.unwrap_or(0.0) / 10000.0;

        let entries: Vec<DragonTigerEntry> = data.iter().map(|item| {
            // TRADE_DATE 形如 "2026-07-31 00:00:00"，只保留日期部分
            let date = item.trade_date.clone().unwrap_or_default();
            let date = date.split(' ').next().unwrap_or(&date).to_string();
            DragonTigerEntry {
                code: item.security_code.clone().unwrap_or_default(),
                name: item.security_name.clone().unwrap_or_default(),
                date,
                close_price: item.close_price.unwrap_or(0.0),
                pct_change: item.change_rate.unwrap_or(0.0),
                turnover: item.turnover_rate.unwrap_or(0.0),
                amount: to_wan(item.accum_amount),
                amplitude: 0.0,
                net_amount: to_wan(item.net_amount),
                total_buy: to_wan(item.total_buy),
                total_sell: to_wan(item.total_sell),
                lhb_amount_ratio: item.lhb_ratio.unwrap_or(0.0),
                reason: item.reason.clone().unwrap_or_default(),
                buy_seats: vec![],
                sell_seats: vec![],
                institution_net_buy: 0.0,
                hot_money_tags: vec![],
            }
        }).collect();

        Ok(entries)
    }

    // ============ Phase 4: 财务数据 ============

    async fn fetch_financial_indicators(&self, code: &str) -> AppResult<FinancialSummary> {
        // 报表名的坑：`RPT_DMSK_FN_MAININDICATOR` 已下线，东财现在回
        // `code: 9501 报表配置不存在`，`result` 直接是 null。老实现用
        // `unwrap_or_default()` 把它吞成空数组，于是接口恒返 200 + 全空对象，
        // 前端显示一张空表却不报错。所以这里除了换成现役报表，还必须把
        // `success` / `code` / `message` 读出来做显式校验。
        //
        // 现役报表 `RPT_F10_FINANCE_MAINFINADATA` 是 F10 页在用的那张，
        // 注意它的 `source`/`client` 是 HSF10/PC（不是老报表的 WEB/WEB），
        // 两者 schema 完全不同，字段名从 `BASIC_EPS` 这类全称换成了
        // `EPSJB` 这类缩写，金额单位也从万元变成了元。
        let url = format!(
            "http://datacenter.eastmoney.com/securities/api/data/v1/get?reportName=RPT_F10_FINANCE_MAINFINADATA&columns=ALL&filter=(SECURITY_CODE=\"{}\")&pageNumber=1&pageSize=8&sortTypes=-1&sortColumns=REPORT_DATE&source=HSF10&client=PC",
            code
        );

        tracing::debug!("Fetching financial indicators: {}", url);

        #[derive(Deserialize)]
        struct FinResp {
            /// 业务是否成功。报表不存在、参数非法时为 false
            success: Option<bool>,
            /// 0 为正常；9501=报表配置不存在，9201=查询无结果
            code: Option<i32>,
            message: Option<String>,
            /// 失败时为 null
            result: Option<FinResult>,
        }
        #[derive(Deserialize)]
        struct FinResult {
            data: Option<Vec<FinItem>>,
        }
        #[derive(Deserialize)]
        struct FinItem {
            // 请求已按单一 SECURITY_CODE 过滤，响应里的代码字段用不上
            #[serde(rename = "SECURITY_NAME_ABBR")]
            security_name: Option<String>,
            // 老报表回 "2026-06-30"，新报表回 "2026-06-30 00:00:00"，
            // 不截断的话前端表格的「报告期」列会带一串零
            #[serde(rename = "REPORT_DATE")]
            report_date: Option<String>,
            /// 年报 / 中报 / 一季报 / 三季报
            #[serde(rename = "REPORT_TYPE")]
            report_type: Option<String>,
            #[serde(rename = "EPSJB", default)]
            eps: Option<f64>,
            #[serde(rename = "BPS", default)]
            bvps: Option<f64>,
            #[serde(rename = "MGJYXJJE", default)]
            cfps: Option<f64>,
            #[serde(rename = "ROEJQ", default)]
            roe: Option<f64>,
            #[serde(rename = "ZZCJLL", default)]
            roa: Option<f64>,
            #[serde(rename = "XSMLL", default)]
            gross_margin: Option<f64>,
            #[serde(rename = "XSJLL", default)]
            net_margin: Option<f64>,
            // 以下三个金额字段上游单位是**元**，模型与前端都按**万元**渲染，
            // 必须在这一层 ÷10000，否则茅台中报营收会显示成 922 万亿
            #[serde(rename = "TOTALOPERATEREVE", default)]
            revenue: Option<f64>,
            #[serde(rename = "TOTALOPERATEREVETZ", default)]
            revenue_yoy: Option<f64>,
            #[serde(rename = "PARENTNETPROFIT", default)]
            net_profit: Option<f64>,
            #[serde(rename = "PARENTNETPROFITTZ", default)]
            net_profit_yoy: Option<f64>,
            #[serde(rename = "KCFJCXSYJLR", default)]
            deducted_profit: Option<f64>,
            #[serde(rename = "KCFJCXSYJLRTZ", default)]
            deducted_profit_yoy: Option<f64>,
            #[serde(rename = "ZCFZL", default)]
            debt_ratio: Option<f64>,
            #[serde(rename = "LD", default)]
            current_ratio: Option<f64>,
            #[serde(rename = "SD", default)]
            quick_ratio: Option<f64>,
        }

        // 同龙虎榜：datacenter.eastmoney.com 返回非合法 gzip，强制 identity 并本地解析
        let resp_text = self.client
            .get(&url)
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .send()
            .await?
            .text()
            .await?;
        let resp: FinResp = serde_json::from_str(&resp_text)?;

        // 上游明确报错（报表不存在 / 参数非法 / 无结果）时抛出去，
        // 而不是静默降级成空列表——否则前端永远显示空表，用户以为查到了
        // 只是没数据，实际是接口早就坏了。
        if let Some(false) = resp.success {
            return Err(AppError::Internal(format!(
                "东财财务接口返回失败 (code={:?}, message={:?})",
                resp.code,
                resp.message.as_deref().unwrap_or("无")
            )));
        }
        if let Some(c) = resp.code {
            if c != 0 {
                return Err(AppError::Internal(format!(
                    "东财财务接口错误码 {} ({})",
                    c,
                    resp.message.as_deref().unwrap_or("无")
                )));
            }
        }

        let data = resp.result.and_then(|r| r.data).unwrap_or_default();
        if data.is_empty() {
            return Err(AppError::NotFound(format!(
                "未找到 {} 的财务数据（股票可能不存在或尚未披露报告）",
                code
            )));
        }

        let indicators: Vec<FinancialIndicator> = data.iter().map(|item| FinancialIndicator {
            report_date: item
                .report_date
                .as_deref()
                .map(|d| if d.len() >= 10 { d[..10].to_string() } else { d.to_string() })
                .unwrap_or_default(),
            report_type: item.report_type.clone(),
            eps: item.eps,
            bvps: item.bvps,
            cfps: item.cfps,
            roe: item.roe,
            roa: item.roa,
            gross_margin: item.gross_margin,
            net_margin: item.net_margin,
            revenue: item.revenue.map(|v| v / 10_000.0),
            revenue_yoy: item.revenue_yoy,
            net_profit: item.net_profit.map(|v| v / 10_000.0),
            net_profit_yoy: item.net_profit_yoy,
            deducted_profit: item.deducted_profit.map(|v| v / 10_000.0),
            deducted_profit_yoy: item.deducted_profit_yoy,
            debt_ratio: item.debt_ratio,
            current_ratio: item.current_ratio,
            quick_ratio: item.quick_ratio,
        }).collect();

        let latest_report = indicators.first().map(|i| i.report_date.clone()).unwrap_or_default();

        Ok(FinancialSummary {
            stock_code: code.to_string(),
            stock_name: data.first().and_then(|d| d.security_name.clone()).unwrap_or_default(),
            latest_report_date: latest_report,
            pe: None,
            pb: None,
            ps: None,
            total_market_cap: None,
            float_market_cap: None,
            indicators,
        })
    }
}

impl EastmoneyFetcher {
    /// 东财取三大指数（旧路径，保留为主源）
    async fn fetch_indices_em(&self) -> AppResult<Vec<IndexSummary>> {
        let index_codes = ["1.000001", "0.399001", "0.399006"];
        let mut out = Vec::with_capacity(3);
        for secid in &index_codes {
            let url = format!(
                "{}/get?secid={}&fltt=2&fields=f43,f44,f45,f46,f47,f48,f57,f58,f169,f170",
                EM_BASE, secid
            );
            // 单个指数失败不该拖垮整块，用 continue 而非 `?`
            let resp: EmResponse<EmQuoteData> = match self.client.get(&url).send().await {
                Ok(r) => match r.json().await {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::debug!(secid = %secid, error = %e, "指数 JSON 解析失败");
                        continue;
                    }
                },
                Err(e) => {
                    tracing::debug!(secid = %secid, error = %e, "指数请求失败");
                    continue;
                }
            };
            if let Some(data) = resp.data {
                out.push(IndexSummary {
                    code: data.code.clone(),
                    name: data.name.clone(),
                    price: data.price,
                    pct_change: data.pct_change,
                    change: data.change,
                    volume: data.volume,
                    amount: data.amount,
                });
            }
        }
        Ok(out)
    }

    /// 腾讯行情取三大指数。
    ///
    /// 返回 GBK 文本 `v_sh000001="1~上证指数~000001~3697.52~...~";`，
    /// 字段位（实测）：1=名称 2=代码 3=最新 4=昨收 5=今开 31=涨跌额
    /// 32=涨跌幅% 33=最高 34=最低 35=`价/量(手)/额(元)` 36=成交量(手)。
    /// 成交额取 35 的第三段，与 37 字段（万元）交叉验证一致。
    async fn fetch_indices_tencent(&self) -> AppResult<Vec<IndexSummary>> {
        // 腾讯 secid 是 sh/sz/bj + 6 位代码，与东财的 `市场.代码` 不同
        let pairs = [
            ("sh000001", "000001"),
            ("sz399001", "399001"),
            ("sz399006", "399006"),
        ];
        let qs: Vec<&str> = pairs.iter().map(|(q, _)| *q).collect();
        let url = format!("http://qt.gtimg.cn/q={}", qs.join(","));

        let bytes = self.client.get(&url).send().await?.bytes().await?;
        let text = decode_gbk(&bytes);

        let mut out = Vec::with_capacity(3);
        for (q, code) in pairs {
            let Some(fields) = extract_tencent_fields(&text, q) else {
                continue;
            };
            let num = |i: usize| -> f64 {
                fields.get(i).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0)
            };
            // fields[35] = "价/成交量(手)/成交额(元)"
            let amount = fields
                .get(35)
                .and_then(|s| s.split('/').nth(2))
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);
            // 腾讯的成交量单位是「手」，换算成股（1 手 = 100 股）与东财口径对齐
            let volume = num(36) * 100.0;

            out.push(IndexSummary {
                code: code.to_string(),
                name: fields.get(1).map(|s| s.to_string()).unwrap_or_default(),
                price: num(3),
                pct_change: num(32),
                change: num(31),
                volume,
                amount,
            });
        }
        Ok(out)
    }

    /// 获取沪深A股涨跌家数
    async fn fetch_up_down_count(&self) -> AppResult<(u32, u32, u32)> {
        // 涨幅榜的沪/深分页 URL 早年写过但从未被使用（涨跌家数改走下面的总览接口），
        // 这里不再保留死代码，避免 url 变量名让读者以为还有第二路数据源。
        let overview_path = "/api/qt/ulist.np/get?                             fields=f1,f2,f3,f4,f6,f12,f13,f104,f105,f106&secids=1.000001,0.399001";

        #[derive(Deserialize)]
        struct OverviewResp {
            data: Option<OverviewData>,
        }
        #[derive(Deserialize)]
        struct OverviewData {
            diff: Option<Vec<OverviewItem>>,
        }
        #[derive(Deserialize)]
        struct OverviewItem {
            #[serde(rename = "f104", default)]
            up_count: u32,
            #[serde(rename = "f105", default)]
            down_count: u32,
            #[serde(rename = "f106", default)]
            flat_count: u32,
        }

        let body = crate::push2::get_text_any_node(&self.client, overview_path).await?;
        let resp: OverviewResp = serde_json::from_str(&body)?;

        if let Some(data) = resp.data {
            if let Some(diff) = data.diff {
                let (up, down, flat) = diff.iter().fold((0u32, 0u32, 0u32), |acc, item| {
                    (acc.0 + item.up_count, acc.1 + item.down_count, acc.2 + item.flat_count)
                });
                return Ok((up, down, flat));
            }
        }

        Ok((0, 0, 0))
    }
}

/// 按股票代码去重，保留首次出现
///
/// datacenter 的分页在排序不稳定时可能返回重复行（详见
/// `fetch_stock_industry_map` 的注释）。重复会直接污染下游统计：成分股等权
/// 平均会被同一只股票重复加权，市场宽度也会重复计数。这里做最后一道兜底。
fn dedup_by_code(rows: Vec<(String, String, String)>) -> Vec<(String, String, String)> {
    let mut seen = std::collections::HashSet::new();
    rows.into_iter()
        .filter(|(code, _, _)| seen.insert(code.clone()))
        .collect()
}


/// 把响应字节按 GBK 解码。
///
/// 腾讯/新浪的行情接口都是 GBK，用 `resp.text()` 会因缺 charset 头而按 UTF-8
/// 解出乱码，所以必须显式按字节解。
fn decode_gbk(bytes: &[u8]) -> String {
    let (cow, _, had_errors) = encoding_rs::GBK.decode(bytes);
    if had_errors {
        tracing::debug!("GBK 解码出现替换字符，上游可能已返回 UTF-8");
    }
    cow.into_owned()
}

/// 从腾讯行情文本里抠出某个 secid 的字段数组。
///
/// 形如 `v_sh000001="1~上证指数~...";`，可能有多个、可能带换行。
fn extract_tencent_fields<'a>(text: &'a str, secid: &str) -> Option<Vec<&'a str>> {
    let marker = format!("v_{secid}=\"");
    let start = text.find(&marker)? + marker.len();
    let rest = &text[start..];
    let end = rest.find("\";").or_else(|| rest.find('"'))?;
    Some(rest[..end].split('~').collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tencent_index_line() {
        // 真实响应片段（2026-10-26 收盘）
        let text = r#"v_sh000001="1~上证指数~000001~3697.52~3690.48~3671.00~432794318~0~0~0.00~0~0.00~0~0.00~0~0.00~0~0.00~0~0.00~0~0.00~0~0.00~0~0.00~0~0.00~0~~20261026161400~7.04~0.19~3699.01~3664.82~3697.52/432794318/746528025716~432794318~74652803~0.89~16.07~~3699.01~3664.82~0.93~579327.61~647365.80~0.00~-1~-1~0.99~0~3680.41~~~~~~74652802.5716~0.0000~0~ ~ZS~-6.84~-0.90~~~~4258.86~3574.27~-2.63~-5.48~-3.06~48574";"#;
        let f = extract_tencent_fields(text, "sh000001").expect("应能抠出字段");
        assert_eq!(f[1], "上证指数");
        assert_eq!(f[2], "000001");
        assert!((f[3].parse::<f64>().unwrap() - 3697.52).abs() < 1e-6);
        // 32 = 涨跌幅、31 = 涨跌额
        assert!((f[32].parse::<f64>().unwrap() - 0.19).abs() < 1e-6);
        assert!((f[31].parse::<f64>().unwrap() - 7.04).abs() < 1e-6);
        // 35 的第三段是成交额（元）≈ 7465 亿
        let amt: f64 = f[35].split('/').nth(2).unwrap().parse().unwrap();
        assert!((amt / 1e8 - 7465.28).abs() < 0.5, "成交额换算成亿应约 7465");
    }

    #[test]
    fn extracts_second_symbol_from_multi_line_payload() {
        let text = "v_sh000001=\"1~上证指数~000001~3697.52\";\nv_sz399001=\"51~深证成指~399001~11534.38\";\n";
        let a = extract_tencent_fields(text, "sh000001").unwrap();
        assert_eq!(a[1], "上证指数");
        let b = extract_tencent_fields(text, "sz399001").unwrap();
        assert_eq!(b[1], "深证成指");
    }

    #[test]
    fn missing_symbol_returns_none() {
        let text = "v_sh000001=\"1~上证指数~000001\";\n";
        assert!(extract_tencent_fields(text, "sz399006").is_none());
    }

    #[test]
    fn gbk_bytes_decoded_to_chinese() {
        let (bytes, _, _) = encoding_rs::GBK.encode("v_sh000001=\"1~上证指数\";");
        let s = decode_gbk(&bytes);
        assert!(s.contains("上证指数"));
    }


    #[test]
    fn test_parse_sina_kline_field_order_and_volume_unit() {
        // 新浪是 开/高/低/收，腾讯是 开/收/高/低 —— 顺序照搬腾讯会张冠李戴
        let raw = r#"[{"day":"2026-07-27","open":"1308.000","high":"1310.000","low":"1279.580","close":"1289.500","volume":"3199000"},
                      {"day":"2026-07-28","open":"1299.000","high":"1320.000","low":"1289.520","close":"1320.000","volume":"5313500"}]"#;
        let k = EastmoneyFetcher::parse_sina_kline(raw, "600519").unwrap();
        assert_eq!(k.len(), 2);
        assert_eq!(k[0].date, "2026-07-27");
        assert_eq!(k[0].open, 1308.0);
        assert_eq!(k[0].high, 1310.0);
        assert_eq!(k[0].low, 1279.58);
        assert_eq!(k[0].close, 1289.5);
        // 3199000 股 → 31990 手，与腾讯口径对齐
        assert_eq!(k[0].volume, 31990.0);
        // 首根没有前收盘，涨跌幅应为「缺失」而不是 0
        assert!(k[0].pct_change.is_none());
        // (1320 - 1289.5) / 1289.5 ≈ 2.365%
        let pct = k[1].pct_change.unwrap();
        assert!((pct - 2.3652).abs() < 0.001, "pct={pct}");
    }

    /// 实测板块快照分页
    ///
    /// 这两个断言专门盯「分页逻辑本身」，不只是接口通不通：
    /// - `>= 400` 保证 `total` 算出来的页数没把数据截短（曾因 URL 少了 `/get`
    ///   导致首页解析失败、静默回落慢路径，日报口径从「官方涨跌幅」变成
    ///   「等权平均」却看不出报错）；
    /// - 去重后仍 >= 400 保证跨页没有重复计数。
    #[tokio::test]
    #[ignore = "需要外网访问"]
    async fn test_board_snapshots_live() {
        let f = EastmoneyFetcher::new();
        let snaps = f.fetch_board_snapshots().await.unwrap();
        assert!(
            snaps.len() >= 400,
            "板块数量偏少，分页可能只取到首页：{}",
            snaps.len()
        );
        let uniq: std::collections::HashSet<&str> =
            snaps.iter().map(|s| s.bk_code.as_str()).collect();
        assert!(
            uniq.len() >= 400,
            "去重后板块数量偏少，疑似跨页重复：{}",
            uniq.len()
        );
    }

    /// 实测全市场快表分页
    ///
    /// 同 `test_board_snapshots_live`：盯的是分页完整性，不只是连通性。
    /// 沪深两市当前约 5200 只，低于 4000 说明并发补页漏了页。
    #[tokio::test]
    #[ignore = "需要外网访问"]
    async fn test_market_snapshot_live() {
        let f = EastmoneyFetcher::new();
        let bars = f.fetch_market_snapshot().await.unwrap();
        assert!(
            bars.len() >= 4000,
            "覆盖个股数偏少，分页可能漏页：{}",
            bars.len()
        );
        let uniq: std::collections::HashSet<&str> =
            bars.iter().map(|b| b.code.as_str()).collect();
        assert_eq!(
            uniq.len(),
            bars.len(),
            "全市场快表出现重复代码，分页顺序或去重有问题"
        );
    }

    #[test]
    fn test_parse_sina_kline_skips_unparsable_rows() {
        let raw = r#"[{"day":"2026-07-27","open":"-","high":"-","low":"-","close":"-","volume":"-"}]"#;
        assert!(EastmoneyFetcher::parse_sina_kline(raw, "600519").unwrap().is_empty());
    }

    /// 实测新浪兜底源是否可用
    ///
    /// 标记为 ignore：需要外网。腾讯被 WAF 拦截后日报还能不能跑，全看这条路径，
    /// 排查行情源问题时用 `cargo test -p fetcher -- --ignored` 手动跑。
    #[tokio::test]
    #[ignore = "需要外网访问"]
    async fn test_sina_kline_live() {
        let f = EastmoneyFetcher::new();
        let (k, status) = f.fetch_kline_sina("600519", "day", 30).await.unwrap();
        assert_eq!(status, KlineStatus::Data, "正常标的应判为 Data");
        assert!(!k.is_empty(), "新浪日线返回空");
        // 至少除首根外都应有涨跌幅
        assert!(k.iter().filter(|x| x.pct_change.is_some()).count() > 1);
        // 成交量已换算成手，个股应在合理量级而不是「股」的百万级
        assert!(k.last().unwrap().volume > 0.0 && k.last().unwrap().volume < 1e7);
    }

    /// 实测「日线经主源或兜底源」能拿到数据
    ///
    /// **这个测试不断言腾讯可用**：它调的是 `fetch_kline`，主源拿不到时会静默
    /// 回落到新浪，所以腾讯挂了它照样通过。早先它叫 `test_tencent_kline_live`、
    /// 断言文案写「腾讯日线返回空」，于是查主源故障时它永远绿着，却在暗示
    /// 「腾讯是好的」——纯粹的误导。要验证主源本身，看下面
    /// [`test_tencent_primary_source_live`]。
    #[tokio::test]
    #[ignore = "需要外网访问"]
    async fn test_kline_live_via_either_source() {
        let f = EastmoneyFetcher::new();
        let k = f
            .fetch_kline("600519", &KlinePeriod::Daily, None, None, Some(30))
            .await
            .unwrap();
        assert!(!k.is_empty(), "腾讯与新浪都没拿到日线");
    }

    /// 实测腾讯主源本身是否可用（不回落兜底源）
    ///
    /// 存在的意义：主源被 WAF 拦截时（实测 `web.ifzq.gtimg.cn` 返回 HTTP 501）
    /// 上面的测试依然全绿，因为新浪兜住了。这条直接打主源，把「腾讯挂了」这件
    /// 事暴露出来——它是熔断逻辑的输入，挂了就意味着全市场日线改走不复权口径。
    #[tokio::test]
    #[ignore = "需要外网访问"]
    async fn test_tencent_primary_source_live() {
        let f = EastmoneyFetcher::new();
        match f
            .fetch_kline_tencent("600519", "sh600519", "day", 30)
            .await
        {
            TencentOutcome::Ok(k) => assert!(!k.is_empty(), "腾讯主源返回空"),
            // 不 assert 失败：主源不可用是**允许的状态**，兜底源会接管，熔断也是
            // 为此设计的。这里只把它明确记下来，让人一眼看出走的是哪条路
            TencentOutcome::SourceDown => panic!(
                "腾讯主源不可用（网络错误或响应体不是合法 JSON，多半是 WAF 拦截）。\
                 这不是缺陷——日线会自动走新浪兜底源，但拿到的将是不复权口径"
            ),
        }
    }

    /// 实测「未上市」能否与「抓取失败」区分开
    ///
    /// 这是覆盖率提示不再误报的关键：301688 格林生物已出现在行业归属表里，
    /// 但从未交易过。若判成 Error，每份日报都会挂一条假的数据缺失告警。
    #[tokio::test]
    #[ignore = "需要外网访问"]
    async fn test_kline_status_distinguishes_unlisted() {
        let f = EastmoneyFetcher::new();

        // 用 999999 而不是「某个还没上市的新股代码」：后者会腐烂——本测试最早用
        // 301688，该股于 2026-09-02 上市当天这个断言就挂了。999 段只用于指数，
        // 不在任何股票代码段内（沪 600/601/603/605/688/900，深 000/001/002/003/
        // 300/301/200，北 4xx/8xx/920），不可能被分配。
        let (k, status) = f
            .fetch_kline_with_status("999999", &KlinePeriod::Daily, None, None, Some(10))
            .await
            .unwrap();
        assert!(k.is_empty());
        assert_eq!(
            status,
            KlineStatus::NoData,
            "未上市标的应判为 NoData，报成 Error 会让日报误报数据缺失"
        );

        let (k, status) = f
            .fetch_kline_with_status("600519", &KlinePeriod::Daily, None, None, Some(10))
            .await
            .unwrap();
        assert!(!k.is_empty());
        assert_eq!(status, KlineStatus::Data);
    }

    /// 主源覆盖不到目标日期时必须回落到兜底源
    ///
    /// 回归测试：2026-07-24 需要 43 根日线时，腾讯对北交所 920 段返回的窗口
    /// 覆盖不到该日（343 只 100% 落空），而新浪同参数下全部含该日。改动前只要
    /// 主源返回非空 K 线就直接采纳，调用方只能把「主源窗口不够」解释成
    /// 「当日停牌」，一天凭空多出三百多只假停牌。
    #[tokio::test]
    #[ignore = "需要外网访问"]
    async fn test_kline_status_falls_back_when_primary_lacks_target_date() {
        let f = EastmoneyFetcher::new();
        let (k, status) = f
            .fetch_kline_with_status(
                "920000",
                &KlinePeriod::Daily,
                None,
                Some("2026-07-24"),
                Some(43),
            )
            .await
            .unwrap();

        assert_eq!(status, KlineStatus::Data);
        assert!(
            k.iter().any(|x| x.date == "2026-07-24"),
            "主源覆盖不到目标日期时应回落到新浪，而不是返回一份不含该日的数据"
        );
    }

    /// 财务接口的回归护栏
    ///
    /// 曾经的故障：`RPT_DMSK_FN_MAININDICATOR` 被东财下线后返回 `code: 9501
    /// 报表配置不存在`，而旧实现用 `unwrap_or_default()` 抹掉了这个错误，
    /// 接口恒返 200 + 全空对象，前端只显示一张空表，线上静默失效了很久。
    /// 这个测试同时盯住三件事：报表还活着、金额单位换算正确、日期已截断。
    #[tokio::test]
    #[ignore = "需要外网访问"]
    async fn test_financial_indicators_live() {
        let f = EastmoneyFetcher::new();
        let fin = f.fetch_financial_indicators("600519").await.unwrap();

        assert_eq!(fin.stock_name, "贵州茅台");
        assert!(
            fin.indicators.len() >= 4,
            "至少应拿到 4 个报告期，实际 {} 个",
            fin.indicators.len()
        );

        let latest = &fin.indicators[0];
        // 上游现在回 "2026-06-30 00:00:00"，不截断会污染前端表格与 rowKey
        assert!(
            latest.report_date.len() == 10 && latest.report_date.parse::<chrono::NaiveDate>().is_ok(),
            "report_date 应为 YYYY-MM-DD，实际 {:?}",
            latest.report_date
        );
        // 2026 中报期别
        assert!(latest.report_type.is_some(), "REPORT_TYPE 缺失，报表 schema 可能又变了");

        // 单位护栏：上游给元，模型与前端按万元渲染。茅台 2026 中报营收约 922.78 亿
        // = 9,227,807 万元。漏掉 ÷10000 会变成 92,278,072,083「万元」。
        let revenue_wan = latest.revenue.expect("营收缺失");
        assert!(
            (9_000_000.0..=9_500_000.0).contains(&revenue_wan),
            "营收应在 900 万~950 万万元之间（约 922.78 亿），实际 {revenue_wan} —— 单位换算可能丢了"
        );
        assert_eq!(latest.eps, Some(35.57), "每股收益应为 35.57 元");
        // 报告期应严格降序
        let dates: Vec<&str> = fin.indicators.iter().map(|i| i.report_date.as_str()).collect();
        let mut sorted = dates.clone();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        assert_eq!(dates, sorted, "报告期未按日期降序返回");
    }

    /// 上游「查无此股」必须抛错，不能静默返回空对象
    #[tokio::test]
    #[ignore = "需要外网访问"]
    async fn test_financial_indicators_reports_missing_stock() {
        let f = EastmoneyFetcher::new();
        let err = f
            .fetch_financial_indicators("999999")
            .await
            .expect_err("不存在的代码应返回错误，而不是空的 FinancialSummary");
        assert!(
            matches!(err, appcore::AppError::NotFound(_) | appcore::AppError::Internal(_)),
            "错误类型应能被 API 层映射成 4xx/5xx，实际 {err:?}"
        );
    }

    /// 新浪返回的字面量 `null` 无法被 K 线解析器处理，必须在解析前单独拦一道
    #[test]
    fn test_parse_sina_kline_rejects_literal_null() {
        // 若 parse_sina_kline 哪天改成能吃下 null，fetch_kline_sina 里的
        // 显式判空就会变成死代码，这个测试会先于线上出问题而失败。
        assert!(EastmoneyFetcher::parse_sina_kline("null", "301688").is_err());
        assert!(EastmoneyFetcher::parse_sina_kline("null\n", "301688").is_err());
        // 空数组是合法响应，解析成功但没有 K 线
        assert!(EastmoneyFetcher::parse_sina_kline("[]", "301688")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn test_tencent_kline_url_uses_https() {
        // http 侧现在会 302 到 https，被限流时甚至直接返回 WAF 页面
        let url = EastmoneyFetcher::tencent_kline_url("sh600519", "day", 30);
        assert!(url.starts_with("https://"), "url={url}");
        assert!(url.contains("param=sh600519,day,,,30,qfq"));
    }

    #[test]
    fn test_em_num_accepts_number_and_string() {
        let n = Some(serde_json::Value::Number(
            serde_json::Number::from_f64(1.5).unwrap(),
        ));
        let s = Some(serde_json::Value::String("2.5".to_string()));
        let dash = Some(serde_json::Value::String("-".to_string()));
        assert_eq!(em_num(&n), 1.5);
        assert_eq!(em_num(&s), 2.5);
        assert_eq!(em_num(&dash), 0.0);
        assert_eq!(em_num(&None), 0.0);
    }

    #[test]
    fn test_em_str_opt_handles_mixed_shapes() {
        // 领涨股代码在同一份响应里可能既是字符串又是数字
        let s = Some(serde_json::Value::String("300413".to_string()));
        let n = Some(serde_json::Value::Number(600371.into()));
        let empty = Some(serde_json::Value::String("".to_string()));
        assert_eq!(em_str_opt(&s).as_deref(), Some("300413"));
        assert_eq!(em_str_opt(&n).as_deref(), Some("600371"));
        assert_eq!(em_str_opt(&empty), None);
        assert_eq!(em_str_opt(&None), None);
    }

    #[test]
    fn test_parse_tencent_kline_day() {
        let raw = r#"{"code":0,"data":{"sh600519":{"qfqday":[["2026-07-27","1308.000","1289.500","1308.000","1279.580","31990.000"],["2026-07-28","1299.000","1320.000","1320.000","1289.520","53135.000"]]}}}"#;
        let klines =
            EastmoneyFetcher::parse_tencent_kline(raw, "600519", "sh600519", "day").unwrap();
        assert_eq!(klines.len(), 2);
        assert_eq!(klines[0].date, "2026-07-27");
        assert_eq!(klines[0].open, 1308.0);
        assert_eq!(klines[0].close, 1289.5);
        assert_eq!(klines[0].high, 1308.0);
        assert_eq!(klines[0].low, 1279.58);
        assert_eq!(klines[0].volume, 31990.0);
        assert_eq!(klines[0].stock_code, "600519");
        assert!(klines[0].pct_change.is_none());
        assert_eq!(klines[0].amount, 0.0);
        assert!(klines[0].turnover.is_none());
        assert_eq!(
            klines[1].pct_change.unwrap(),
            (1320.0 - 1289.5) / 1289.5 * 100.0
        );
    }

    #[test]
    fn test_parse_tencent_kline_week() {
        let raw = r#"{"code":0,"data":{"sh600519":{"qfqweek":[["2026-07-03","1169.000","1194.450","1215.520","1151.010","234098.000"]]}}}"#;
        let klines =
            EastmoneyFetcher::parse_tencent_kline(raw, "600519", "sh600519", "week").unwrap();
        assert_eq!(klines.len(), 1);
        assert_eq!(klines[0].date, "2026-07-03");
        assert_eq!(klines[0].close, 1194.45);
    }

    #[test]
    fn test_parse_tencent_kline_fallback_to_raw_day() {
        let raw = r#"{"code":0,"data":{"sh600519":{"day":[["2026-07-27","1308.000","1289.500","1308.000","1279.580","31990.000"]]}}}"#;
        let klines =
            EastmoneyFetcher::parse_tencent_kline(raw, "600519", "sh600519", "day").unwrap();
        assert_eq!(klines.len(), 1);
        assert_eq!(klines[0].date, "2026-07-27");
    }

    #[test]
    fn test_parse_tencent_kline_error_code_empty() {
        let raw = r#"{"code":-1,"data":null}"#;
        let klines =
            EastmoneyFetcher::parse_tencent_kline(raw, "600519", "sh600519", "day").unwrap();
        assert!(klines.is_empty());
    }

    #[test]
    fn test_tencent_secid_prefix() {
        assert_eq!(EastmoneyFetcher::tencent_secid("600519"), "sh600519");
        assert_eq!(EastmoneyFetcher::tencent_secid("688981"), "sh688981");
        assert_eq!(EastmoneyFetcher::tencent_secid("000001"), "sz000001");
        assert_eq!(EastmoneyFetcher::tencent_secid("300750"), "sz300750");
        assert_eq!(EastmoneyFetcher::tencent_secid("430047"), "bj430047");
        // 沪市 B 股是 900xxx，不能跟着 9 一起划到北交所
        assert_eq!(EastmoneyFetcher::tencent_secid("900901"), "sh900901");
    }

    #[test]
    fn test_tencent_secid_beijing_920() {
        // 北交所 2024 年起启用 920 代码段。挂 sz 时腾讯与新浪都返回空，
        // 曾导致一次全量抓取整体丢失 343 只（占 6.1%）。
        assert_eq!(EastmoneyFetcher::tencent_secid("920819"), "bj920819");
        assert_eq!(EastmoneyFetcher::tencent_secid("920002"), "bj920002");
        // 老代码段同样归北交所
        assert_eq!(EastmoneyFetcher::tencent_secid("830799"), "bj830799");
        assert_eq!(EastmoneyFetcher::tencent_secid("873223"), "bj873223");
    }

    #[test]
    fn test_parse_tencent_minute_kline() {
        let raw = r#"{"code":0,"msg":"","data":{"sz000001":{"m5":[["202607271340","11.14","11.15","11.15","11.13","12347.00",{},"0.6363"],["202607271345","11.15","11.14","11.15","11.14","14315.68",{},"0.7377"]]}}}"#;
        let klines =
            EastmoneyFetcher::parse_tencent_minute_kline(raw, "000001", "sz000001", "m5").unwrap();
        assert_eq!(klines.len(), 2);
        assert_eq!(klines[0].date, "2026-07-27 13:40");
        assert_eq!(klines[0].open, 11.14);
        assert_eq!(klines[0].close, 11.15);
        assert_eq!(klines[0].high, 11.15);
        assert_eq!(klines[0].low, 11.13);
        assert_eq!(klines[0].volume, 12347.0);
        assert_eq!(klines[0].amount, 0.6363);
        assert_eq!(klines[0].stock_code, "000001");
        assert!(klines[0].pct_change.is_none());
        assert_eq!(
            klines[1].pct_change.unwrap(),
            (11.14 - 11.15) / 11.15 * 100.0
        );
    }

    #[test]
    fn test_parse_tencent_minute_kline_error_code_empty() {
        let raw = r#"{"code":-1,"msg":"","data":null}"#;
        let klines =
            EastmoneyFetcher::parse_tencent_minute_kline(raw, "000001", "sz000001", "m5").unwrap();
        assert!(klines.is_empty());
    }

    #[test]
    fn test_parse_tencent_minute_kline_missing_period_empty() {
        // data 节点存在但缺少请求的 m5 周期
        let raw = r#"{"code":0,"msg":"","data":{"sz000001":{"m1":[["202607271340","11.14","11.15","11.15","11.13","12347.00",{},"0.6363"]]}}}"#;
        let klines =
            EastmoneyFetcher::parse_tencent_minute_kline(raw, "000001", "sz000001", "m5").unwrap();
        assert!(klines.is_empty());
    }

    #[test]
    fn test_dedup_by_code_keeps_first() {
        let rows = vec![
            ("600519".to_string(), "贵州茅台".to_string(), "白酒Ⅱ".to_string()),
            ("000001".to_string(), "平安银行".to_string(), "股份制银行Ⅱ".to_string()),
            // 分页漂移导致的重复行，行业归属可能不同，保留首次出现
            ("600519".to_string(), "贵州茅台".to_string(), "食品饮料".to_string()),
        ];
        let out = dedup_by_code(rows);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, "600519");
        assert_eq!(out[0].2, "白酒Ⅱ", "重复行应保留首次出现的行业归属");
        assert_eq!(out[1].0, "000001");
    }

    #[test]
    fn test_dedup_by_code_noop_on_unique() {
        let rows: Vec<(String, String, String)> = (0..50)
            .map(|i| (format!("{:06}", i), format!("股票{i}"), "半导体".to_string()))
            .collect();
        assert_eq!(dedup_by_code(rows).len(), 50);
    }

    #[test]
    fn test_dedup_by_code_empty() {
        assert!(dedup_by_code(Vec::new()).is_empty());
    }

    #[test]
    fn test_format_minute_datetime() {
        assert_eq!(
            EastmoneyFetcher::format_minute_datetime("202608051025"),
            "2026-08-05 10:25"
        );
        // 非标准格式原样返回
        assert_eq!(
            EastmoneyFetcher::format_minute_datetime("2026-07-27"),
            "2026-07-27"
        );
    }
}
