use async_trait::async_trait;
use appcore::models::*;
use appcore::AppResult;

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
