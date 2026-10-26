//! 同花顺金融数据 API（fuyao）客户端。
//!
//! 定位：**东财 push2 集群不可达时的第三路数据源**，只覆盖涨停复盘 / 龙虎榜
//! 这几个「东财一挂就整块空白」的页面级功能。
//!
//! 与东财实现的本质差别（决定了它不是简单替换）：
//!
//! | 维度 | 东财 push2（现主源） | 同花顺 fuyao（本模块） |
//! |---|---|---|
//! | 连板天数 | 恒 `1`（字段压根没取） | 真实 `continue_day_cnt`，如 `5天4板` → 4 |
//! | 封单额 | 恒 `0.0` | 真实 `seal_money`（含峰值 `max_seal_money`） |
//! | 涨停判定 | 写死 `pct_change >= 9.5` | 由上游按板块规则给出，创业板/科创/北交所/ST 全对 |
//! | 涨停时间 | 无 | `HH:MM` |
//! | 涨停原因 | 无 | `limit_up_reason`（题材串） |
//! | 炸板 | 用「涨幅 5~9.5%」冒充 | 真实炸板池 + `open_times` + `turnover_ratio_pct` |
//!
//! 即：**回落同花顺的数据质量比东财主源更高**，不再是「降级」，只是拿不到
//! 部分字段（成交额/换手率/流通市值在涨停池里没有，这里按 0 留空，与东财
//! 占位 0 的既有行为一致，前端本来就把 0 当「无数据」）。
//!
//! 契约要点（见 https://fuyao.aicubes.cn/docs/api-reference/）：
//! - Base URL `https://fuyao.aicubes.cn`，鉴权头是 **`X-api-key`**，不是 `Bearer`。
//! - **业务错误一律 HTTP 200**，靠信封里的 `code` 表达（`0` 为成功，
//!   限流是 `4001`，参数错是 `1002/1003`，鉴权错是 `2001`）。
//!   因此绝不能只看 HTTP 状态码，必须解 `code`，否则会被当成「拿到了空数据」。
//! - 涨跌幅字段为百分数原值；金额字段单位为**元**，而本项目模型统一用**万元**，
//!   故所有金额都要 `/ 1e4`。

use appcore::models::{BrokenBoardStock, BrokerSeat, DragonTigerEntry, LimitDownStock, LimitUpStock};
use appcore::{AppError, AppResult};
use reqwest::Client;
use serde::Deserialize;
use std::sync::OnceLock;

/// 同花顺金融数据 API 根地址
const FUYAO_BASE: &str = "https://fuyao.aicubes.cn";

/// 涨停池单页上限（接口约束 1..=200）
const PAGE_SIZE: u32 = 200;
/// 分页上限保护：涨停/跌停/炸板池单日实测量级远小于此，防上游异常时死循环
const MAX_PAGES: u32 = 5;

/// 同花顺 fuyao 数据采集器。
///
/// 与 `EastmoneyFetcher` 一样需要 `&self` 跨线程共享，故保持无内部可变状态；
/// key 读一次就缓存（`OnceLock`），避免每次请求都查环境变量。
pub struct FuyaoFetcher {
    client: Client,
    api_key: OnceLock<String>,
}

impl Default for FuyaoFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl FuyaoFetcher {
    pub fn new() -> Self {
        // 走全仓共享 Client：连接池与超时统一管理（10s 超时 / 3s 连接超时）
        Self {
            client: crate::headers::shared_client().clone(),
            api_key: OnceLock::new(),
        }
    }

    /// 用指定 key 构造（测试与显式配置用；正常路径读环境变量 `FUYAO_API_KEY`）
    pub fn with_api_key(key: impl Into<String>) -> Self {
        let s = Self::new();
        let _ = s.api_key.set(key.into());
        s
    }

    /// 本数据源是否可用（有 key）。
    ///
    /// 没有 key 时上层应直接跳过本数据源，别发注定 401 的请求。
    pub fn is_configured(&self) -> bool {
        !self.key().is_empty()
    }

    fn key(&self) -> &str {
        self.api_key
            .get_or_init(|| std::env::var("FUYAO_API_KEY").unwrap_or_default())
            .as_str()
    }

    /// 发一个带鉴权的 GET，并把统一信封解成 `data` 部分。
    ///
    /// 这里刻意把「HTTP 失败」「业务 code 失败」「data 为空」三件事分开报错：
    /// 上游限流（`code=4001`）是 HTTP 200，如果只判状态码会静默拿到空列表，
    /// 上层就会显示「今日无涨停」——比报错更有害。
    async fn get_data<T: for<'de> Deserialize<'de>>(
        &self,
        path_and_query: &str,
    ) -> AppResult<T> {
        let key = self.key();
        if key.is_empty() {
            return Err(AppError::InvalidInput(
                "未配置同花顺 API Key（环境变量 FUYAO_API_KEY），该数据源已跳过".into(),
            ));
        }

        let url = format!("{FUYAO_BASE}{path_and_query}");
        let resp = self
            .client
            .get(&url)
            .header("X-api-key", key)
            .send()
            .await
            .map_err(|e| AppError::Network(format!("同花顺请求失败: {e}")))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| AppError::Network(format!("同花顺响应读取失败: {e}")))?;

        if !status.is_success() {
            let snippet: String = body.chars().take(200).collect();
            return Err(AppError::Network(format!(
                "同花顺 HTTP {status}: {snippet}"
            )));
        }

        let envelope: Envelope<T> = serde_json::from_str(&body)
            .map_err(|e| AppError::Parse(format!("同花顺响应解析失败: {e}")))?;

        match envelope.code {
            0 => envelope
                .data
                .ok_or_else(|| AppError::Parse("同花顺返回成功但没有 data".into())),
            // 4001 = 触发限流（HTTP 仍为 200）。转成 Network 让上层按「稍后重试」处理，
            // 而不是当成「今天没有涨停」。
            4001 => Err(AppError::Network(
                "同花顺触发限流（code=4001），请降低并发后重试".into(),
            )),
            2001 | 2003 => Err(AppError::InvalidInput(format!(
                "同花顺 API Key 无效（code={}），请检查 FUYAO_API_KEY",
                envelope.code
            ))),
            c => Err(AppError::Parse(format!(
                "同花顺业务错误 code={c}: {}",
                envelope.message
            ))),
        }
    }

    /// 翻页抓一个「池」类接口（涨停/跌停/炸板共用一个 `item[]` 骨架）。
    ///
    /// **必须显式传 `date_ms`**：实测不传时上游按「服务端当前自然日」取数，
    /// 盘中跌停池/炸板池因为这些集合还在动态变化，会返回 `total=0`，
    /// 页面会显示「今日无炸板」这种明显错误的结果。传最近交易日零点则稳定。
    async fn fetch_pool<T>(&self, endpoint: &str, sort: &str) -> AppResult<Vec<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        self.fetch_pool_on(endpoint, sort, recent_trading_day_ms()).await
    }

    /// 指定交易日抓池（`date_ms` 为该日上海时区零点毫秒戳）。
    async fn fetch_pool_on<T>(
        &self,
        endpoint: &str,
        sort: &str,
        date_ms: i64,
    ) -> AppResult<Vec<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        let mut out = Vec::new();
        for page in 1..=MAX_PAGES {
            let path = format!(
                "{endpoint}?page={page}&size={PAGE_SIZE}&sort_field={sort}&sort_dir=desc&date_ms={date_ms}"
            );
            let data: PagedData<T> = self.get_data(&path).await?;
            let got = data.item.len();
            let pages = data.pagination.as_ref().map(|p| p.pages).unwrap_or(1);
            out.extend(data.item);
            // 最后一页 / 空页就停；上游 pages 缺失时靠 got==0 兜底
            if page >= pages || got == 0 {
                break;
            }
        }
        Ok(out)
    }

    /// 抓池并做「当日空 → 退回上一交易日」的回退。
    ///
    /// 为什么需要：盘前/盘中调跌停池与炸板池，当日 `total` 就是 0
    /// （集合还在动态形成），而这个 0 在页面上和「今天真的一只都没有」
    /// 长得一模一样。只有涨停池盘前会有数据（沿用上一交易日结果）。
    /// 两害相权：退回上一交易日，让页面显示「最近一个有数据的交易日」，
    /// 比显示一个凭空出现的「无炸板」更有用——后者会让人误判情绪修复。
    async fn fetch_pool_with_fallback<T, F>(&self, endpoint: &str, sort: &str, map: F) -> AppResult<Vec<T>>
    where
        T: for<'de> Deserialize<'de>,
        F: Fn(Vec<serde_json::Value>) -> Vec<T>,
    {
        // 这里不收 T，先取原始 JSON，才能对两次尝试用同一套映射
        let today = recent_trading_day_ms();
        let raw: Vec<serde_json::Value> = self.fetch_pool_on(endpoint, sort, today).await?;
        if !raw.is_empty() {
            return Ok(map(raw));
        }
        let prev = prev_trading_day_ms(today);
        tracing::info!(%endpoint, "同花顺当日池为空，回退到上一交易日");
        let raw = self.fetch_pool_on(endpoint, sort, prev).await?;
        Ok(map(raw))
    }

    /// 涨停池（含连板天数、封单额、涨停时间、涨停原因）
    pub async fn fetch_limit_ups(&self) -> AppResult<Vec<LimitUpStock>> {
        let rows: Vec<LimitUpItem> = self
            .fetch_pool("/api/a-share/special-data/limit-up-pool", "continue_day_cnt")
            .await?;
        Ok(rows.into_iter().map(LimitUpItem::into_model).collect())
    }

    /// 跌停池（当日为空时退回上一交易日）
    pub async fn fetch_limit_downs(&self) -> AppResult<Vec<LimitDownStock>> {
        let rows: Vec<LimitDownStock> = self
            .fetch_pool_with_fallback(
                "/api/a-share/special-data/limit-down-pool",
                "last_limit_time",
                |v| {
                    v.into_iter()
                        .filter_map(|x| serde_json::from_value::<LimitDownItem>(x).ok())
                        .map(LimitDownItem::into_model)
                        .collect()
                },
            )
            .await?;
        Ok(rows)
    }

    /// 炸板池（真实炸板集合 + 开板次数 + 换手率；当日为空时退回上一交易日）
    pub async fn fetch_broken_boards(&self) -> AppResult<Vec<BrokenBoardStock>> {
        let rows: Vec<BrokenBoardStock> = self
            .fetch_pool_with_fallback(
                "/api/a-share/special-data/limit-break-pool",
                "open_times",
                |v| {
                    v.into_iter()
                        .filter_map(|x| serde_json::from_value::<BrokenBoardItem>(x).ok())
                        .map(BrokenBoardItem::into_model)
                        .collect()
                },
            )
            .await?;
        Ok(rows)
    }

    /// 龙虎榜榜单。`date` 为 `yyyy-MM-dd`，`None` 时上游回退到最近交易日。
    ///
    /// 上游同一只股票可能同时出现在「当日榜」与「3 日榜」，`range_days`
    /// 区分。这里只保留 `range_days == 1` 的当日榜：龙虎榜页面按票聚合时，
    /// 混入 3 日榜会把两个口径的净买入加到一起，得出既不是当日也不是 3 日
    /// 的第三个数（东财实现也只看当日榜）。
    pub async fn fetch_dragon_tiger(&self, date: Option<&str>) -> AppResult<Vec<DragonTigerEntry>> {
        let mut path = String::from("/api/a-share/special-data/dragon-tiger-list?board_type=all");
        if let Some(d) = date {
            path.push_str(&format!("&date={d}"));
        }
        let data: DragonTigerData = self.get_data(&path).await?;
        let trade_date = data.trade_date.unwrap_or_default();

        Ok(data
            .stock_items
            .into_iter()
            .filter(|it| it.range_days.unwrap_or(1) == 1)
            .map(|it| it.into_model(&trade_date))
            .collect())
    }
}

// ============ 统一信封 ============

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    message: String,
    // 注意：不能写 `#[serde(default)] data: Option<T>` —— 那会要求 `T: Default`，
    // 而 `Option` 本身缺字段即为 `None`，不需要 default。
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct Pagination {
    #[serde(default)]
    pages: u32,
}

#[derive(Debug, Deserialize)]
struct PagedData<T> {
    #[serde(default)]
    pagination: Option<Pagination>,
    item: Vec<T>,
}

/// 最近一个交易日的上海时区零点毫秒戳。
///
/// 简化处理：周一到周五取当天，周六取前一天（周五），周日取前两天。
/// 不查交易日历（节假日会退到「节前最后一个工作日」，此时上游返回空池，
/// 与东财主源的表现一致，不会引入新的错误说法）。
fn recent_trading_day_ms() -> i64 {
    use chrono::{Datelike, Duration, FixedOffset, TimeZone, Utc, Weekday};

    let cst = FixedOffset::east_opt(8 * 3600).expect("UTC+8 合法");
    let today = Utc::now().with_timezone(&cst).date_naive();
    let back = match today.weekday() {
        Weekday::Sat => 1,
        Weekday::Sun => 2,
        _ => 0,
    };
    let d = today - Duration::days(back);
    cst.from_local_datetime(&d.and_hms_opt(0, 0, 0).expect("00:00:00 合法"))
        .single()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0)
}

/// 上一个交易日的上海时区零点毫秒戳（往回退 1 天，遇周末继续退）。
fn prev_trading_day_ms(from_ms: i64) -> i64 {
    use chrono::{Datelike, Duration, FixedOffset, TimeZone, Timelike, Utc, Weekday};

    let cst = FixedOffset::east_opt(8 * 3600).expect("UTC+8 合法");
    let base = Utc
        .timestamp_millis_opt(from_ms)
        .single()
        .map(|dt| dt.with_timezone(&cst))
        .unwrap_or_else(|| Utc::now().with_timezone(&cst));
    let mut d = base.date_naive() - Duration::days(1);
    while matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
        d -= Duration::days(1);
    }
    let _ = base.hour(); // 保留 Timelike 引入的显式意图
    cst.from_local_datetime(&d.and_hms_opt(0, 0, 0).expect("00:00:00 合法"))
        .single()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0)
}

/// 元 → 万元（项目模型统一单位）。上游给 null 时按 0 处理。
fn wan(yuan: Option<f64>) -> f64 {
    yuan.unwrap_or(0.0) / 1e4
}

/// 「5天4板」→ 4；「首板」→ 1。
///
/// 上游 `continue_day_cnt` 已经给出数值，优先用它；这里只在上游缺失时兜底，
/// 且只认「N板」——「5天4板」里 `天` 在前，直接抓第一个数字会错成 5。
fn board_count_from_text(text: &str) -> u32 {
    text.split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<u32>().ok())
        .last()
        .unwrap_or(1)
}

// ============ 涨停池 ============

#[derive(Debug, Deserialize)]
struct LimitUpItem {
    #[serde(default)]
    ticker: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    last_price: f64,
    #[serde(default)]
    price_change_ratio_pct: f64,
    #[serde(default)]
    limit_up_time: Option<String>,
    #[serde(default)]
    limit_up_reason: Option<String>,
    #[serde(default)]
    continue_day_text: String,
    #[serde(default)]
    continue_day_cnt: Option<u32>,
    #[serde(default)]
    seal_money: Option<f64>,
}

impl LimitUpItem {
    fn into_model(self) -> LimitUpStock {
        // 上游只给「涨停时间」一个时刻，首次/最后无法区分，都填它；
        // 封板强度需要成交额，涨停池不返回，保持 None。
        let t = self.limit_up_time.map(normalize_hhmm);
        LimitUpStock {
            code: self.ticker,
            name: self.name,
            price: self.last_price,
            pct_change: self.price_change_ratio_pct,
            consecutive_days: self
                .continue_day_cnt
                .unwrap_or_else(|| board_count_from_text(&self.continue_day_text)),
            first_limit_time: t.clone(),
            last_limit_time: t,
            break_count: 0,
            seal_volume: 0.0,
            seal_amount: wan(self.seal_money),
            seal_strength: None,
            amount: 0.0,
            turnover: 0.0,
            float_market_cap: 0.0,
            industry: None,
            reason: self.limit_up_reason.filter(|s| !s.trim().is_empty()),
            amplitude: 0.0,
        }
    }
}

/// `09:34` → `09:34:00`（项目模型注释约定 `HH:MM:SS`）
fn normalize_hhmm(t: String) -> String {
    if t.len() == 5 && t.contains(':') {
        format!("{t}:00")
    } else {
        t
    }
}

// ============ 跌停池 ============

#[derive(Debug, Deserialize)]
struct LimitDownItem {
    #[serde(default)]
    ticker: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    last_price: f64,
    #[serde(default)]
    price_change_ratio_pct: f64,
    #[serde(default)]
    turnover_ratio_pct: f64,
}

impl LimitDownItem {
    fn into_model(self) -> LimitDownStock {
        LimitDownStock {
            code: self.ticker,
            name: self.name,
            price: self.last_price,
            pct_change: self.price_change_ratio_pct,
            // 跌停池不返回连板天数与封单额，按「未知」置 0（与东财占位口径一致）
            consecutive_days: 0,
            seal_amount: 0.0,
            amount: 0.0,
            turnover: self.turnover_ratio_pct,
            industry: None,
        }
    }
}

// ============ 炸板池 ============

#[derive(Debug, Deserialize)]
struct BrokenBoardItem {
    #[serde(default)]
    ticker: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    last_price: f64,
    #[serde(default)]
    price_change_ratio_pct: f64,
    #[serde(default)]
    open_times: u32,
    #[serde(default)]
    turnover_ratio_pct: f64,
    #[serde(default)]
    turnover: Option<f64>,
}

impl BrokenBoardItem {
    fn into_model(self) -> BrokenBoardStock {
        BrokenBoardStock {
            code: self.ticker,
            name: self.name,
            price: self.last_price,
            pct_change: self.price_change_ratio_pct,
            // 炸板池没有「最高涨幅」。用 0 而不是拿当前涨幅顶替 ——
            // 东财实现正是用当前涨幅冒充，才让炸板筛选逻辑失真。
            max_pct: 0.0,
            break_count: self.open_times,
            amount: wan(self.turnover),
            turnover: self.turnover_ratio_pct,
            industry: None,
        }
    }
}

// ============ 龙虎榜 ============

#[derive(Debug, Deserialize)]
struct DragonTigerData {
    #[serde(default)]
    trade_date: Option<String>,
    #[serde(default)]
    stock_items: Vec<DragonTigerItem>,
}

#[derive(Debug, Deserialize)]
struct DragonTigerItem {
    #[serde(default)]
    ticker: String,
    #[serde(default)]
    name: String,
    /// 小数形式（0.0999 = 9.99%）
    #[serde(default)]
    change: f64,
    #[serde(default)]
    net_value: Option<f64>,
    #[serde(default)]
    buy_value: Option<f64>,
    #[serde(default)]
    sell_value: Option<f64>,
    #[serde(default)]
    net_rate: Option<f64>,
    #[serde(default)]
    limit_reason: Option<String>,
    #[serde(default)]
    range_days: Option<u32>,
    #[serde(default)]
    amount: Option<f64>,
    #[serde(default)]
    org_net_value: Option<f64>,
}

impl DragonTigerItem {
    fn into_model(self, trade_date: &str) -> DragonTigerEntry {
        DragonTigerEntry {
            code: self.ticker,
            name: self.name,
            date: trade_date.to_string(),
            // 上游不返回收盘价，只有涨跌幅；用 0 占位而不是反推一个假价格
            close_price: 0.0,
            pct_change: self.change * 100.0,
            // 换手率/振幅该接口不提供（成交额有，换手率没有），保持 0
            turnover: 0.0,
            amount: wan(self.amount),
            amplitude: 0.0,
            net_amount: wan(self.net_value),
            total_buy: wan(self.buy_value),
            total_sell: wan(self.sell_value),
            // net_rate 是小数，项目模型要百分数
            lhb_amount_ratio: self.net_rate.unwrap_or(0.0) * 100.0,
            reason: self
                .limit_reason
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "龙虎榜".to_string()),
            // 席位明细是另一个接口（dragon-tiger-stock-institution），列表接口不返回；
            // 页面只要榜单汇总，这里留空而不是伪造
            buy_seats: Vec::<BrokerSeat>::new(),
            sell_seats: Vec::<BrokerSeat>::new(),
            // 机构净买入有字段可用，从「元」换到「万元」
            institution_net_buy: wan(self.org_net_value),
            hot_money_tags: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_count_prefers_trailing_number() {
        // 「5天4板」的连板数是 4，不是第一个数字 5 —— 这是最容易写错的地方
        assert_eq!(board_count_from_text("5天4板"), 4);
        assert_eq!(board_count_from_text("2连板"), 2);
        assert_eq!(board_count_from_text("首板"), 1);
        assert_eq!(board_count_from_text(""), 1);
    }

    #[test]
    fn hhmm_gets_seconds() {
        assert_eq!(normalize_hhmm("09:34".into()), "09:34:00");
        // 已经是完整时间就不动
        assert_eq!(normalize_hhmm("09:34:12".into()), "09:34:12");
    }

    #[test]
    fn yuan_to_wan() {
        // 浮点除法有尾差，用容差比较
        assert!((wan(Some(123_456_789.12)) - 12345.678_912).abs() < 1e-6);
        assert_eq!(wan(None), 0.0);
    }

    #[test]
    fn parses_limit_up_envelope() {
        let body = r#"{"code":0,"message":"success","data":{"timestamp":1,
            "pagination":{"total":1,"pages":1,"size":50,"page":1},
            "item":[{"thscode":"603986.SH","ticker":"603986","name":"兆易创新",
              "is_st":false,"last_price":118.23,"price_change_ratio_pct":10.0008,
              "limit_up_time":"09:34","limit_up_reason":"存储芯片",
              "continue_day_text":"2连板","continue_day_cnt":2,
              "seal_money":123456789.12,"max_seal_money":234567890.12}]}}"#;
        let env: Envelope<PagedData<LimitUpItem>> = serde_json::from_str(body).unwrap();
        assert_eq!(env.code, 0);
        let item = env.data.unwrap().item.into_iter().next().unwrap();
        let m = item.into_model();
        assert_eq!(m.code, "603986");
        assert_eq!(m.consecutive_days, 2);
        assert_eq!(m.first_limit_time.as_deref(), Some("09:34:00"));
        assert_eq!(m.reason.as_deref(), Some("存储芯片"));
        // 封单额从「元」换算到「万元」
        assert!((m.seal_amount - 12345.678_912).abs() < 1e-6);
    }

    #[test]
    fn limit_up_falls_back_to_text_when_cnt_missing() {
        let body = r#"{"ticker":"000001","name":"平安银行","continue_day_text":"5天4板"}"#;
        let item: LimitUpItem = serde_json::from_str(body).unwrap();
        assert_eq!(item.into_model().consecutive_days, 4);
    }

    #[test]
    fn rejects_business_error_even_on_http_200() {
        // 这是最关键的一条：限流/鉴权错误是 HTTP 200，只能靠 code 识别。
        // 若被当成「成功但空列表」，页面会显示「今日无涨停」。
        let body = r#"{"code":4001,"message":"rate limited","data":null}"#;
        let env: Envelope<PagedData<LimitUpItem>> = serde_json::from_str(body).unwrap();
        assert_eq!(env.code, 4001);
        assert!(env.data.is_none());
    }

    #[test]
    fn dragon_tiger_scales_units_and_skips_three_day_rank() {
        let body = r#"{"code":0,"message":"success","data":{
            "trade_date":"2026-10-26","count":2,"stock_count":1,
            "stock_items":[
              {"ticker":"002407","name":"多氟多","change":0.09994,"net_value":1786253128.23,
               "net_rate":0.11901893,"buy_value":2674755016.05,"sell_value":888501887.82,
               "range_days":1,"amount":5000000000},
              {"ticker":"002407","name":"多氟多","change":0.09994,"net_value":999,
               "range_days":3}
            ]}}"#;
        let env: Envelope<DragonTigerData> = serde_json::from_str(body).unwrap();
        let data = env.data.unwrap();
        let date = data.trade_date.clone().unwrap();
        let rows: Vec<_> = data
            .stock_items
            .into_iter()
            .filter(|it| it.range_days.unwrap_or(1) == 1)
            .map(|it| it.into_model(&date))
            .collect();
        // 3 日榜被剔除，只剩当日榜
        assert_eq!(rows.len(), 1);
        let e = &rows[0];
        assert_eq!(e.date, "2026-10-26");
        assert!((e.pct_change - 9.994).abs() < 1e-6);
        assert!((e.net_amount - 178_625.312_823).abs() < 1e-6);
        assert!((e.lhb_amount_ratio - 11.901_893).abs() < 1e-6);
    }

    /// 联调用：需真实 key，默认跳过。
    /// `FUYAO_API_KEY=xxx cargo test -p fetcher -- --ignored live_fuyao`
    #[tokio::test]
    #[ignore = "需要外网 + FUYAO_API_KEY"]
    async fn live_fuyao_pools() {
        let f = FuyaoFetcher::new();
        if !f.is_configured() {
            eprintln!("未配置 FUYAO_API_KEY，跳过");
            return;
        }
        let ups = f.fetch_limit_ups().await.expect("涨停池");
        println!("涨停 {} 只", ups.len());
        assert!(!ups.is_empty(), "交易日应有涨停");
        // 连板天数必须来自上游，而不是东财那种恒 1
        println!("最高连板 {} 板", ups.iter().map(|s| s.consecutive_days).max().unwrap_or(0));
        assert!(ups.iter().any(|s| !s.name.is_empty()));

        let downs = f.fetch_limit_downs().await.expect("跌停池");
        println!("跌停 {} 只", downs.len());

        let broken = f.fetch_broken_boards().await.expect("炸板池");
        println!("炸板 {} 只", broken.len());

        let lhb = f.fetch_dragon_tiger(None).await.expect("龙虎榜");
        println!("龙虎榜 {} 条", lhb.len());
        if let Some(first) = lhb.first() {
            println!("样例: {} {} 净买入 {:.2} 万", first.code, first.name, first.net_amount);
        }
    }

    #[test]
    fn broken_board_does_not_fake_max_pct() {
        let body = r#"{"ticker":"000001","name":"平安银行","last_price":12.5,
            "price_change_ratio_pct":7.6,"open_times":3,"turnover_ratio_pct":8.1,
            "turnover":1250000000}"#;
        let item: BrokenBoardItem = serde_json::from_str(body).unwrap();
        let m = item.into_model();
        assert_eq!(m.break_count, 3);
        // 不用当前涨幅冒充最高涨幅
        assert_eq!(m.max_pct, 0.0);
        assert!((m.amount - 125_000.0).abs() < 1e-6);
    }
}
