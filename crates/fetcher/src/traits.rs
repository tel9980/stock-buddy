use async_trait::async_trait;
use appcore::models::*;
use appcore::AppResult;

/// 日线抓取结果状态
///
/// 「没拿到数据」有三种完全不同的含义，返回值上却都表现为空 `Vec`：
/// 前两种是市场本来就没有该标的当日的数据，报成抓取失败会让用户误以为日报残缺。
/// 实测 5642 只成分股里约 19 只属前两类（10 只未上市、7 只停牌），
/// 若不分类，每份日报都会挂一条「数据可能不完整」的假警报。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KlineStatus {
    /// 拿到了 K 线。注意**可能不含请求的日期**——那说明该日停牌，或当天尚未上市，
    /// 由调用方按最早一根 K 线的日期区分（见 `sentiment::outcome_for_missing_date`）
    Data,
    /// 行情源没有该标的的任何 K 线：未上市 / 已退市 / 代码不存在
    NoData,
    /// 请求失败：网络错误、被 WAF 拦截、返回内容无法解析
    Error,
}

/// 数据采集统一接口
#[async_trait]
pub trait DataFetcher: Send + Sync {
    /// 获取个股实时行情
    async fn fetch_quote(&self, code: &str) -> AppResult<Quote>;

    /// 批量获取实时行情
    async fn fetch_quotes(&self, codes: &[&str]) -> AppResult<Vec<Quote>>;

    /// 获取 K 线数据
    async fn fetch_kline(
        &self,
        code: &str,
        period: &KlinePeriod,
        start: Option<&str>,
        end: Option<&str>,
        limit: Option<u32>,
    ) -> AppResult<Vec<Kline>>;

    /// 获取 K 线并区分「无此标的 / 当日停牌 / 请求失败」
    ///
    /// 三者返回值上都表现为空，但只有最后一种需要告警。默认实现按「空即无数据」
    /// 兜底，无法区分未上市与请求失败；`EastmoneyFetcher` 借助新浪对未知标的
    /// 返回字面量 `null` 的特性做了精确实现。
    async fn fetch_kline_with_status(
        &self,
        code: &str,
        period: &KlinePeriod,
        start: Option<&str>,
        end: Option<&str>,
        limit: Option<u32>,
    ) -> AppResult<(Vec<Kline>, KlineStatus)> {
        let k = self.fetch_kline(code, period, start, end, limit).await?;
        let status = if k.is_empty() {
            KlineStatus::NoData
        } else {
            KlineStatus::Data
        };
        Ok((k, status))
    }

    /// 主源当前是否处于熔断状态（仅主备双源的采集器会有真值）
    ///
    /// 存在的意义是让上层告警能区分两件**处置完全相反**的事：
    ///
    /// | 现象 | 真实原因 | 该怎么处理 |
    /// |---|---|---|
    /// | `tripped == true` | 主源不可用，已整体切兜底 | 等主源恢复，无需调参 |
    /// | `tripped == false` | 兜底源被限流或网络抖动 | 要降并发、错峰重试 |
    ///
    /// 两者在统计上长得一模一样（都是「一批股票没拿到」），不区分就只能打一条
    /// 「疑似限流或 WAF 拦截」，把已经处理掉的情况和真正需要干预的情况混为一谈。
    ///
    /// 没有主备之分的采集器（如纯新浪实现）恒为 false。
    fn primary_source_tripped(&self) -> bool {
        false
    }

    /// 获取板块排行
    async fn fetch_sector_ranks(&self, sector_type: &str) -> AppResult<Vec<SectorRank>>;

    /// 获取板块成分股
    async fn fetch_sector_stocks(&self, sector_code: &str, page: u32, page_size: u32) -> AppResult<(Vec<SectorStock>, u32)>;

    /// 获取大盘总览
    async fn fetch_market_overview(&self) -> AppResult<MarketOverview>;

    // ============ Phase 4: 资金流向 ============

    /// 获取个股资金流向历史
    async fn fetch_capital_flow(&self, code: &str, limit: Option<u32>) -> AppResult<Vec<CapitalFlow>>;

    /// 获取行业板块资金流排行
    async fn fetch_sector_capital_flows(&self) -> AppResult<Vec<SectorCapitalFlow>>;

    /// 获取北向资金流向历史
    async fn fetch_north_bound_flow(&self, limit: Option<u32>) -> AppResult<Vec<NorthBoundFlow>>;

    /// 获取北向持股季度汇总（2024-08-19 起交易所仅按季度披露）
    async fn fetch_north_bound_holdings(&self) -> AppResult<Vec<NorthBoundHoldings>>;

    /// 获取沪深两市主力资金（实时）
    async fn fetch_market_capital_flow(&self) -> AppResult<MarketCapitalFlow>;

    /// 获取融资融券数据
    async fn fetch_margin_trading(&self, limit: Option<u32>) -> AppResult<Vec<MarginTrading>>;

    // ============ Phase 4: 涨停复盘 ============

    /// 获取涨停股列表
    async fn fetch_limit_ups(&self) -> AppResult<Vec<LimitUpStock>>;

    /// 获取跌停股列表
    async fn fetch_limit_downs(&self) -> AppResult<Vec<LimitDownStock>>;

    /// 获取炸板股列表
    async fn fetch_broken_boards(&self) -> AppResult<Vec<BrokenBoardStock>>;

    /// 获取涨停复盘全景
    async fn fetch_limit_up_review(&self) -> AppResult<LimitUpReview>;

    // ============ Phase 4: 龙虎榜 ============

    /// 获取龙虎榜上榜列表（指定日期）
    async fn fetch_dragon_tiger(&self, date: Option<&str>) -> AppResult<Vec<DragonTigerEntry>>;

    // ============ Phase 4: 财务数据 ============

    /// 获取个股财务指标
    async fn fetch_financial_indicators(&self, code: &str) -> AppResult<FinancialSummary>;
}
