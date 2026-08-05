use anyhow::Result;
use appcore::models::{AiAnalysisHistory, AiHistoryItem, Kline, NewAiHistory, Stock};
use sqlx::SqlitePool;

/// 数据访问层
#[derive(Clone)]
pub struct Repository {
    pool: SqlitePool,
}

impl Repository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn from_database(db: &crate::Database) -> Self {
        Self::new(db.pool().clone())
    }

    // --- Stock ---

    pub async fn upsert_stock(&self, stock: &Stock) -> Result<()> {
        sqlx::query(
            "INSERT INTO stocks (code, name, market, industry, list_date, status)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(code) DO UPDATE SET
                name=excluded.name, market=excluded.market,
                industry=excluded.industry, status=excluded.status"
        )
        .bind(&stock.code)
        .bind(&stock.name)
        .bind(format!("{:?}", stock.market))
        .bind(&stock.industry)
        .bind(&stock.list_date)
        .bind(format!("{:?}", stock.status))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_stock(&self, code: &str) -> Result<Option<Stock>> {
        let row: Option<(String, String, String, Option<String>, Option<String>, String)> =
            sqlx::query_as("SELECT code, name, market, industry, list_date, status FROM stocks WHERE code = ?")
                .bind(code)
                .fetch_optional(&self.pool)
                .await?;

        Ok(row.map(|(code, name, market_str, industry, list_date, status_str)| Stock {
            code,
            name,
            market: match market_str.as_str() {
                "Sh" => appcore::models::Market::Sh,
                "Bj" => appcore::models::Market::Bj,
                _ => appcore::models::Market::Sz,
            },
            industry,
            list_date,
            status: match status_str.as_str() {
                "Delisted" => appcore::models::StockStatus::Delisted,
                "Suspended" => appcore::models::StockStatus::Suspended,
                _ => appcore::models::StockStatus::Active,
            },
        }))
    }

    // --- KLine ---

    pub async fn save_klines(&self, klines: &[Kline]) -> Result<()> {
        for k in klines {
            sqlx::query(
                "INSERT INTO kline_daily (stock_code, date, open, close, high, low, volume, amount, pct_change, turnover)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(stock_code, date) DO UPDATE SET
                    open=excluded.open, close=excluded.close, high=excluded.high,
                    low=excluded.low, volume=excluded.volume, amount=excluded.amount,
                    pct_change=excluded.pct_change, turnover=excluded.turnover"
            )
            .bind(&k.stock_code)
            .bind(&k.date)
            .bind(k.open)
            .bind(k.close)
            .bind(k.high)
            .bind(k.low)
            .bind(k.volume)
            .bind(k.amount)
            .bind(k.pct_change)
            .bind(k.turnover)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn get_klines(
        &self,
        code: &str,
        start: Option<&str>,
        end: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<Kline>> {
        let limit = limit.unwrap_or(365) as i64;

        let rows: Vec<(String, String, f64, f64, f64, f64, f64, f64, Option<f64>, Option<f64>)> =
            sqlx::query_as(
                "SELECT stock_code, date, open, close, high, low, volume, amount, pct_change, turnover
                 FROM kline_daily
                 WHERE stock_code = ? AND (? IS NULL OR date >= ?) AND (? IS NULL OR date <= ?)
                 ORDER BY date DESC
                 LIMIT ?"
            )
            .bind(code)
            .bind(start)
            .bind(start)
            .bind(end)
            .bind(end)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;

        let mut klines: Vec<Kline> = rows
            .into_iter()
            .map(|(stock_code, date, open, close, high, low, volume, amount, pct_change, turnover)| {
                Kline {
                    stock_code,
                    date,
                    open,
                    close,
                    high,
                    low,
                    volume,
                    amount,
                    pct_change,
                    turnover,
                }
            })
            .collect();
        klines.reverse(); // 返回时间正序
        Ok(klines)
    }

    // --- Watchlist ---

    pub async fn get_watchlist(&self, user_id: &str) -> Result<Vec<String>> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT stock_code FROM watchlist WHERE user_id = ? ORDER BY add_time DESC")
                .bind(user_id)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|(c,)| c).collect())
    }

    pub async fn add_to_watchlist(&self, user_id: &str, code: &str, group: Option<&str>) -> Result<()> {
        sqlx::query(
            "INSERT OR IGNORE INTO watchlist (user_id, stock_code, group_name) VALUES (?, ?, ?)"
        )
        .bind(user_id)
        .bind(code)
        .bind(group.unwrap_or("default"))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_from_watchlist(&self, user_id: &str, code: &str) -> Result<()> {
        sqlx::query("DELETE FROM watchlist WHERE user_id = ? AND stock_code = ?")
            .bind(user_id)
            .bind(code)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // --- AI 分析历史 ---

    /// 保存一条 AI 分析历史，返回自增 id
    pub async fn save_ai_history(&self, record: &NewAiHistory) -> Result<i64> {
        let (id,): (i64,) = sqlx::query_as(
            "INSERT INTO ai_analysis_history
                (user_id, analysis_type, title, stock_code, stock_name, question, content, model)
             VALUES ('default', ?, ?, ?, ?, ?, ?, ?)
             RETURNING id"
        )
        .bind(&record.analysis_type)
        .bind(&record.title)
        .bind(&record.stock_code)
        .bind(&record.stock_name)
        .bind(&record.question)
        .bind(&record.content)
        .bind(&record.model)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    /// 列出 AI 分析历史（可按类型过滤，分页）
    pub async fn list_ai_history(
        &self,
        analysis_type: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<AiHistoryItem>> {
        let limit = limit as i64;
        let offset = offset as i64;

        // 列表只取内容前 600 字符作为摘要，避免一次性拉取大文本
        let base = "SELECT id, analysis_type, title, stock_code, stock_name, question,
                           substr(content, 1, 600) AS summary, model, created_at
                    FROM ai_analysis_history";
        let rows: Vec<(
            i64, String, String, Option<String>, Option<String>, Option<String>,
            String, Option<String>, String,
        )> = if let Some(t) = analysis_type {
            sqlx::query_as(&format!("{base} WHERE analysis_type = ? ORDER BY created_at DESC LIMIT ? OFFSET ?"))
                .bind(t)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?
        } else {
            sqlx::query_as(&format!("{base} ORDER BY created_at DESC LIMIT ? OFFSET ?"))
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?
        };

        Ok(rows
            .into_iter()
            .map(|(id, atype, title, code, name, question, summary, model, created_at)| {
                AiHistoryItem {
                    id,
                    analysis_type: atype,
                    title,
                    stock_code: code,
                    stock_name: name,
                    question,
                    summary: make_summary(&summary),
                    model,
                    created_at,
                }
            })
            .collect())
    }

    /// 获取单条历史详情（完整内容）
    pub async fn get_ai_history(&self, id: i64) -> Result<Option<AiAnalysisHistory>> {
        let row: Option<(
            i64, String, String, Option<String>, Option<String>, Option<String>,
            String, Option<String>, String,
        )> = sqlx::query_as(
            "SELECT id, analysis_type, title, stock_code, stock_name, question, content, model, created_at
             FROM ai_analysis_history WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(id, atype, title, code, name, question, content, model, created_at)| {
            AiAnalysisHistory {
                id,
                user_id: "default".to_string(),
                analysis_type: atype,
                title,
                stock_code: code,
                stock_name: name,
                question,
                content,
                model,
                created_at,
            }
        }))
    }

    /// 删除一条历史
    pub async fn delete_ai_history(&self, id: i64) -> Result<()> {
        sqlx::query("DELETE FROM ai_analysis_history WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

/// 将内容片段转为展示摘要：去除多余空白、截断
fn make_summary(raw: &str) -> String {
    let collapsed: String = raw
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    let collapsed = collapsed.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > 120 {
        let truncated: String = collapsed.chars().take(120).collect();
        format!("{truncated}…")
    } else {
        collapsed
    }
}
