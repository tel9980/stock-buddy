use crate::ai::collector::{
    DataCollector,
    format_quote_json,
    format_kline_summary,
    format_indicators_json,
    format_sr_json,
    format_market_json,
    format_sectors_json,
};
use crate::ai::llm::LlmClient;
use crate::ai::prompts;
use std::sync::Arc;
use tokio::sync::mpsc;

/// AI 分析服务
pub struct AiService {
    llm: LlmClient,
    collector: DataCollector,
}

impl AiService {
    pub fn new(llm: LlmClient, fetcher: Arc<dyn fetcher::DataFetcher>) -> Self {
        Self {
            llm,
            collector: DataCollector::new(fetcher),
        }
    }

    pub fn is_configured(&self) -> bool {
        self.llm.is_configured()
    }

    /// AI 个股诊断（完整响应）
    pub async fn diagnose_stock(&self, code: &str) -> anyhow::Result<String> {
        let ctx = self.collector.collect_for_stock(code).await?;
        let quote_json = format_quote_json(&ctx.quote);
        let kline_summary = format_kline_summary(&ctx.klines);
        let indicators_json = format_indicators_json(&ctx.macd, &ctx.kdj, &ctx.rsi, &ctx.boll, ctx.quote.price);
        let sr_json = format_sr_json(&ctx.support_resistance);
        let market_json = ctx.market
            .as_ref()
            .map(format_market_json)
            .unwrap_or_else(|| "市场数据获取失败".to_string());

        let messages = prompts::diagnosis_prompt(
            &ctx.quote.name, code,
            &quote_json, &kline_summary, &indicators_json, &sr_json, &market_json,
        );

        self.llm.chat(messages).await
    }

    /// AI 个股诊断（流式响应）
    pub async fn diagnose_stock_stream(
        &self,
        code: &str,
    ) -> anyhow::Result<mpsc::Receiver<Result<String, anyhow::Error>>> {
        let ctx = self.collector.collect_for_stock(code).await?;
        let quote_json = format_quote_json(&ctx.quote);
        let kline_summary = format_kline_summary(&ctx.klines);
        let indicators_json = format_indicators_json(&ctx.macd, &ctx.kdj, &ctx.rsi, &ctx.boll, ctx.quote.price);
        let sr_json = format_sr_json(&ctx.support_resistance);
        let market_json = ctx.market
            .as_ref()
            .map(format_market_json)
            .unwrap_or_else(|| "市场数据获取失败".to_string());

        let messages = prompts::diagnosis_prompt(
            &ctx.quote.name, code,
            &quote_json, &kline_summary, &indicators_json, &sr_json, &market_json,
        );

        self.llm.chat_stream(messages).await
    }

    /// AI 市场研判
    pub async fn analyze_market(&self) -> anyhow::Result<String> {
        let (market, sectors) = self.collector.collect_for_market().await?;
        let market_json = format_market_json(&market);
        let sectors_json = format_sectors_json(&sectors);
        let messages = prompts::market_prompt(&market_json, &sectors_json);
        self.llm.chat(messages).await
    }

    /// AI 市场研判（流式）
    pub async fn analyze_market_stream(
        &self,
    ) -> anyhow::Result<mpsc::Receiver<Result<String, anyhow::Error>>> {
        let (market, sectors) = self.collector.collect_for_market().await?;
        let market_json = format_market_json(&market);
        let sectors_json = format_sectors_json(&sectors);
        let messages = prompts::market_prompt(&market_json, &sectors_json);
        self.llm.chat_stream(messages).await
    }

    /// AI 趋势预测
    pub async fn predict_trend(&self, code: &str) -> anyhow::Result<String> {
        let ctx = self.collector.collect_for_stock(code).await?;
        let multi_period = self.collector.collect_multi_period(code).await.unwrap_or_default();
        let quote_json = format_quote_json(&ctx.quote);
        let kline_summary = format_kline_summary(&ctx.klines);
        let indicators_json = format_indicators_json(&ctx.macd, &ctx.kdj, &ctx.rsi, &ctx.boll, ctx.quote.price);

        let messages = prompts::prediction_prompt(
            &ctx.quote.name, code,
            &quote_json, &kline_summary, &indicators_json, &multi_period,
        );

        self.llm.chat(messages).await
    }

    /// AI 趋势预测（流式）
    pub async fn predict_trend_stream(
        &self,
        code: &str,
    ) -> anyhow::Result<mpsc::Receiver<Result<String, anyhow::Error>>> {
        let ctx = self.collector.collect_for_stock(code).await?;
        let multi_period = self.collector.collect_multi_period(code).await.unwrap_or_default();
        let quote_json = format_quote_json(&ctx.quote);
        let kline_summary = format_kline_summary(&ctx.klines);
        let indicators_json = format_indicators_json(&ctx.macd, &ctx.kdj, &ctx.rsi, &ctx.boll, ctx.quote.price);

        let messages = prompts::prediction_prompt(
            &ctx.quote.name, code,
            &quote_json, &kline_summary, &indicators_json, &multi_period,
        );

        self.llm.chat_stream(messages).await
    }

    /// AI 智能问答
    pub async fn chat(&self, question: &str) -> anyhow::Result<String> {
        let context = self.build_chat_context(question).await;
        let messages = prompts::chat_prompt(question, &context);
        self.llm.chat(messages).await
    }

    /// AI 智能问答（流式）
    pub async fn chat_stream(
        &self,
        question: &str,
    ) -> anyhow::Result<mpsc::Receiver<Result<String, anyhow::Error>>> {
        let context = self.build_chat_context(question).await;
        let messages = prompts::chat_prompt(question, &context);
        self.llm.chat_stream(messages).await
    }

    /// 根据问题内容智能构建上下文
    async fn build_chat_context(&self, question: &str) -> String {
        let mut parts = Vec::new();
        let fetcher = self.collector.fetcher();

        // 始终获取市场总览
        if let Ok(market) = fetcher.fetch_market_overview().await {
            parts.push(format!("## 市场总览\n{}", format_market_json(&market)));
        }

        // 尝试从问题中提取股票代码（6位数字）
        if let Some(code) = extract_stock_code(question) {
            if let Ok(ctx) = self.collector.collect_for_stock(&code).await {
                parts.push(format!("## {}({}) 实时行情\n{}", ctx.quote.name, code, format_quote_json(&ctx.quote)));
                parts.push(format!("## 近期K线\n{}", format_kline_summary(&ctx.klines)));
                parts.push(format!("## 技术指标\n{}", format_indicators_json(&ctx.macd, &ctx.kdj, &ctx.rsi, &ctx.boll, ctx.quote.price)));
                parts.push(format!("## 支撑压力位\n{}", format_sr_json(&ctx.support_resistance)));
            }
        }

        // 尝试获取板块数据
        if let Ok(sectors) = fetcher.fetch_sector_ranks("industry").await {
            parts.push(format!("## 板块排行\n{}", format_sectors_json(&sectors)));
        }

        if parts.is_empty() {
            "暂无可用数据".to_string()
        } else {
            parts.join("\n\n")
        }
    }
}

/// 从文本中提取6位股票代码
fn extract_stock_code(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i + 6 <= chars.len() {
        let slice: String = chars[i..i+6].iter().collect();
        if slice.chars().all(|c| c.is_ascii_digit()) {
            let first = chars[i];
            if first == '0' || first == '3' || first == '6' || first == '2' {
                return Some(slice);
            }
        }
        i += 1;
    }
    None
}
