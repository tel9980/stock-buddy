use anyhow::Result;
use appcore::models::{
    AiAnalysisHistory, AiHistoryItem, JobRunInfo, Kline, NewAiHistory, SentimentArchiveEntry,
    SentimentDaily, Stock,
};
use sqlx::SqlitePool;
use std::collections::HashMap;

/// 单条多行 INSERT 里放多少行日线
///
/// 500 个绑定变量，远低于 SQLite 的 `SQLITE_MAX_VARIABLE_NUMBER`（老版本 999，
/// 新版 32766），在两种构建下都安全。再大收益递减，再小则语句数变多。
const BAR_INSERT_CHUNK: usize = 100;

/// 一只股票的日线序列是何时抓的、覆盖到哪一天
///
/// 见 `migrations/004_kline_series.sql` 的完整说明。
#[derive(Debug, Clone)]
pub struct SeriesMeta {
    /// 抓取日期（本地时区 `YYYY-MM-DD`）
    pub fetched_at: String,
    /// 已落库的最早交易日
    pub first_date: String,
    /// 已落库的最晚交易日
    pub last_date: String,
    /// 是否已回溯到上市首日。为 true 时「目标日早于 `first_date`」才能判定为
    /// 「那天还没上市」，否则只是窗口不够长，得加大窗口重抓
    pub complete: bool,
}

/// 待写入的序列抓取记录
#[derive(Debug, Clone)]
pub struct SeriesMetaRow {
    pub stock_code: String,
    pub fetched_at: String,
    pub first_date: String,
    pub last_date: String,
    pub complete: bool,
}

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
    // （save_klines 已删除：全仓无调用者的死代码，且逐条 INSERT 无事务性能极差；
    //   批量写入走下方 save_daily_bars 的多行 INSERT + 分块事务路径。）

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

    pub async fn get_watchlist(&self, user_id: &str) -> Result<Vec<appcore::WatchlistItem>> {
        let rows: Vec<(String, f64, i64)> = sqlx::query_as(
            "SELECT stock_code, cost_price, shares FROM watchlist WHERE user_id = ? ORDER BY add_time DESC"
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(code, cost_price, shares)| appcore::WatchlistItem {
                stock_code: code,
                cost_price,
                shares,
            })
            .collect())
    }

    pub async fn add_to_watchlist(
        &self,
        user_id: &str,
        code: &str,
        group: Option<&str>,
        cost_price: f64,
        shares: i64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT OR IGNORE INTO watchlist (user_id, stock_code, group_name, cost_price, shares) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(user_id)
        .bind(code)
        .bind(group.unwrap_or("default"))
        .bind(cost_price)
        .bind(shares)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_watchlist(
        &self,
        user_id: &str,
        code: &str,
        cost_price: f64,
        shares: i64,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE watchlist SET cost_price = ?, shares = ? WHERE user_id = ? AND stock_code = ?"
        )
        .bind(cost_price)
        .bind(shares)
        .bind(user_id)
        .bind(code)
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

    // ============ 市场情绪日报 ============

    /// 批量写入个股行业归属（东财二级行业口径）
    pub async fn save_stock_industries(&self, rows: &[(String, String, String)]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let now = chrono::Utc::now().to_rfc3339();
        let mut tx = self.pool.begin().await?;
        for (code, name, industry) in rows {
            sqlx::query(
                "INSERT INTO stock_industry (code, name, industry, updated_at)
                 VALUES (?, ?, ?, ?)
                 ON CONFLICT(code) DO UPDATE SET
                    name=excluded.name, industry=excluded.industry, updated_at=excluded.updated_at"
            )
            .bind(code)
            .bind(name)
            .bind(industry)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// 读取全部个股行业归属，返回 (code, name, industry)
    pub async fn get_industry_map(&self) -> Result<Vec<(String, String, String)>> {
        let rows: Vec<(String, Option<String>, String)> =
            sqlx::query_as("SELECT code, name, industry FROM stock_industry")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows
            .into_iter()
            .map(|(code, name, industry)| (code, name.unwrap_or_default(), industry))
            .collect())
    }

    /// 行业全集（去重），即本库个股行业归属表里出现过的全部行业名
    ///
    /// 慢路径的行业榜就是按这个集合聚合出来的，快路径必须过滤到同一个集合，
    /// 否则会静默少掉官方 `RPT_VALUEINDUSTRY_DET` 清单之外的行业
    /// （实测体育Ⅱ 有 3 只真实成分股、东财板块快照里也有 BK1273，
    /// 但不在官方 127 个二级行业里 → 快路径曾因此只有 127 个行业）。
    pub async fn get_industry_names(&self) -> Result<Vec<String>> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT DISTINCT industry FROM stock_industry WHERE industry <> ''")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|(i,)| i).collect())
    }

    /// 已缓存行业归属的条数，用于判断是否需要首次抓取
    pub async fn count_stock_industries(&self) -> Result<i64> {
        let (n,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM stock_industry")
                .fetch_one(&self.pool)
                .await?;
        Ok(n)
    }

    /// 批量写入日线缓存，元素为 (code, date, close, pct_change, amount)
    ///
    /// 按 [`BAR_INSERT_CHUNK`] 行合成一条多行 INSERT，而不是逐行执行。
    /// 启用序列缓存后单次调用的行数从「几千」涨到「几十万」（5624 只 × 120 根
    /// ≈ 67 万行），逐行走 sqlx 的 prepare/bind/execute 一轮要几秒到几十秒，
    /// 合成多行后只有几千条语句，差一个数量级。
    pub async fn save_daily_bars(&self, rows: &[(String, String, f64, f64, f64)]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await?;
        for chunk in rows.chunks(BAR_INSERT_CHUNK) {
            // SQL 里只拼占位符，所有值都走 bind，不存在注入面
            let placeholders = vec!["(?, ?, ?, ?, ?)"; chunk.len()].join(",");
            let sql = format!(
                "INSERT INTO daily_bar_cache (stock_code, date, close, pct_change, amount)
                 VALUES {placeholders}
                 ON CONFLICT(stock_code, date) DO UPDATE SET
                    close=excluded.close, pct_change=excluded.pct_change, amount=excluded.amount"
            );
            let mut q = sqlx::query(&sql);
            for (code, date, close, pct_change, amount) in chunk {
                q = q.bind(code).bind(date).bind(close).bind(pct_change).bind(amount);
            }
            q.execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// 读取某个交易日的日线缓存，返回 (code, close, pct_change, amount)
    pub async fn get_daily_bars(&self, date: &str) -> Result<Vec<(String, f64, f64, f64)>> {
        let rows: Vec<(String, f64, f64, f64)> = sqlx::query_as(
            "SELECT stock_code, close, pct_change, amount FROM daily_bar_cache WHERE date = ?",
        )
        .bind(date)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// 某交易日已缓存的日线条数，用于判断是否已聚合过
    pub async fn count_daily_bars(&self, date: &str) -> Result<i64> {
        let (n,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM daily_bar_cache WHERE date = ?")
                .bind(date)
                .fetch_one(&self.pool)
                .await?;
        Ok(n)
    }

    /// 读取全部日线序列抓取记录，键为股票代码
    ///
    /// 整表读而不是按代码逐个查：这张表最多 5000 多行，一次扫完只要几毫秒，
    /// 而按 5600 个代码分批 `IN` 查询要写分块逻辑，收益是负的。
    pub async fn all_series_meta(&self) -> Result<HashMap<String, SeriesMeta>> {
        let rows: Vec<(String, String, String, String, i64)> = sqlx::query_as(
            "SELECT stock_code, fetched_at, first_date, last_date, complete
             FROM kline_series_meta",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(stock_code, fetched_at, first_date, last_date, complete)| {
                (
                    stock_code,
                    SeriesMeta {
                        fetched_at,
                        first_date,
                        last_date,
                        complete: complete != 0,
                    },
                )
            })
            .collect())
    }

    /// 记录一批股票的序列抓取结果，已存在则覆盖
    pub async fn save_series_meta(&self, rows: &[SeriesMetaRow]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await?;
        for r in rows {
            sqlx::query(
                "INSERT INTO kline_series_meta
                    (stock_code, fetched_at, first_date, last_date, complete)
                 VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT(stock_code) DO UPDATE SET
                    fetched_at=excluded.fetched_at, first_date=excluded.first_date,
                    last_date=excluded.last_date, complete=excluded.complete"
            )
            .bind(&r.stock_code)
            .bind(&r.fetched_at)
            .bind(&r.first_date)
            .bind(&r.last_date)
            .bind(r.complete as i64)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// 已有序列记录的股票数
    pub async fn count_series_meta(&self) -> Result<i64> {
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM kline_series_meta")
            .fetch_one(&self.pool)
            .await?;
        Ok(n)
    }

    /// 取 `limit` 只「序列最久没刷新」的股票，返回 (代码, 当前序列起点)
    ///
    /// 按 `fetched_at` 升序排，等于一个天然的轮转队列：今天刷过的一批会排到末尾，
    /// 明天轮到下一批。调用方据此算出「每天刷多少只」即可滚动覆盖全市场。
    ///
    /// 一并返回 `first_date`，是为了让刷新时能把窗口放到**足够回溯到现有起点**，
    /// 否则刷新反而会缩短这只股票的覆盖范围（见 jobs.rs 里的说明）。
    pub async fn oldest_series(&self, limit: usize) -> Result<Vec<(String, String)>> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT stock_code, first_date FROM kline_series_meta
             ORDER BY fetched_at ASC, stock_code ASC LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// 缓存整份日报（payload 为序列化后的 JSON）
    pub async fn save_sentiment_daily(&self, date: &str, payload: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO sentiment_daily (date, payload, generated_at)
             VALUES (?, ?, ?)
             ON CONFLICT(date) DO UPDATE SET
                payload=excluded.payload, generated_at=excluded.generated_at"
        )
        .bind(date)
        .bind(payload)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 读取缓存的日报，返回 (payload, generated_at)
    pub async fn get_sentiment_daily(&self, date: &str) -> Result<Option<(String, i64)>> {
        let row: Option<(String, i64)> =
            sqlx::query_as("SELECT payload, generated_at FROM sentiment_daily WHERE date = ?")
                .bind(date)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row)
    }

    /// 已归档日报目录，按日期倒序
    ///
    /// `covered_count` / `board_count` 要反序列化 payload 才拿得到。归档条目
    /// 数量级只有几十条，每次全量重算几毫秒，比加冗余列 + 写迁移划算；真到
    /// 归档上千天时再改成冗余列不迟。
    ///
    /// `has_export` 恒为 `false`——是否留存了导出件由文件系统决定，路由层填。
    pub async fn list_sentiment_daily(&self, limit: u32) -> Result<Vec<SentimentArchiveEntry>> {
        let rows: Vec<(String, String, i64)> = sqlx::query_as(
            "SELECT date, payload, generated_at FROM sentiment_daily
             ORDER BY date DESC LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(date, payload, generated_at)| {
                // 某条归档损坏不该让整个目录不可用：统计值退化为 0，条目仍保留，
                // 用户点进去会走到「缓存反序列化失败 → 重算」的既有分支。
                let (covered_count, board_count) =
                    match serde_json::from_str::<SentimentDaily>(&payload) {
                        Ok(r) => (r.breadth.covered_count, r.boards.len()),
                        Err(e) => {
                            tracing::warn!(
                                date = %date,
                                error = %e,
                                "归档日报反序列化失败，目录条目的统计值退化为 0"
                            );
                            (0, 0)
                        }
                    };
                SentimentArchiveEntry {
                    date,
                    generated_at,
                    payload_bytes: payload.len(),
                    covered_count,
                    board_count,
                    has_export: false,
                }
            })
            .collect())
    }

    /// 删除某交易日的归档，返回是否真的删掉了
    pub async fn delete_sentiment_daily(&self, date: &str) -> Result<bool> {
        let r = sqlx::query("DELETE FROM sentiment_daily WHERE date = ?")
            .bind(date)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    // --- job_runs（定时任务可观测） ---

    /// 记录一次定时任务运行
    pub async fn record_job_run(
        &self,
        job: &str,
        date: &str,
        outcome: &str,
        detail: &str,
        elapsed_ms: Option<u64>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO job_runs (job, date, outcome, detail, elapsed_ms, started_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(job)
        .bind(date)
        .bind(outcome)
        .bind(detail)
        .bind(elapsed_ms.map(|v| v as i64))
        .bind(chrono::Utc::now().timestamp())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 某任务最近一次「有实际动作」的运行记录
    ///
    /// 排除 `skipped`：探测式触发每 15 分钟一轮，绝大多数轮次都是「已归档，无需
    /// 重复生成」。若把它当成最近一次运行展示，用户就永远看不到归档到底成没成、
    /// 花了多久——而那才是真正想知道的。让路给手动请求的 skipped 同理，它只是
    /// 说明这一轮没轮到定时任务，不代表归档的状态。
    ///
    /// 一次都没实际跑过时返回 `None`（自动归档刚开启、当日行情尚未落库）。
    pub async fn latest_job_run(&self, job: &str) -> Result<Option<JobRunInfo>> {
        let row: Option<(String, String, Option<String>, Option<i64>, i64)> = sqlx::query_as(
            "SELECT date, outcome, detail, elapsed_ms, started_at FROM job_runs
             WHERE job = ? AND outcome != 'skipped'
             ORDER BY started_at DESC, id DESC LIMIT 1",
        )
        .bind(job)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(
            |(date, outcome, detail, elapsed_ms, started_at)| JobRunInfo {
                job: job.to_string(),
                date,
                outcome,
                detail,
                elapsed_ms: elapsed_ms.map(|v| v as u64),
                started_at,
            },
        ))
    }

    /// 某任务在指定日期是否已有成功记录
    ///
    /// 每日只该跑一次的维护任务用它做幂等——探测式触发每 15 分钟一轮，没有它
    /// 就会一天跑几十遍。
    pub async fn has_successful_run(&self, job: &str, date: &str) -> Result<bool> {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT 1 FROM job_runs WHERE job = ? AND date = ? AND outcome = 'succeeded' LIMIT 1",
        )
        .bind(job)
        .bind(date)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.is_some())
    }

    /// 清理日线缓存，只保留最近 `keep_dates` 个日期，返回删除的日线行数
    ///
    /// 自动归档让这张表每天稳定增长约 5,600 行（年增约 137 万行 / 160 MB）。
    /// 留着不清理，一年后回来看库已经几百 MB。删掉的日期仍能重新生成，只是要
    /// 重抓一次全市场——老归档本身仍在 `sentiment_daily` 里，不受影响。
    ///
    /// ## 启用序列缓存后，顺带清理失效的序列记录
    ///
    /// 序列缓存让这张表从「每个日期一批行」变成「每只股票一整条序列」，
    /// 于是出现了清理与记录不一致的可能：`kline_series_meta` 仍声称覆盖了
    /// 某个旧日期，而那天的日线行已被清掉。上层正是靠 meta 判定「这只股票
    /// 那天没数据」，于是会把**被清掉的数据**误判成**当日停牌**——一个看不见的
    /// 错误。所以这里把起点已被清掉的 meta 一并删掉，让那只股票下次重新抓一次。
    ///
    /// 少数长期停牌股（实测 6 只）的「最近 120 根」会回溯到一年前，是这张表
    /// 日期数的主要来源，也正是会先被清理掉的那部分。
    pub async fn prune_daily_bars(&self, keep_dates: usize) -> Result<u64> {
        // 嵌套一层子查询：SQLite 不允许在 DELETE 的 NOT IN 子查询里直接写 LIMIT
        let r = sqlx::query(
            "DELETE FROM daily_bar_cache WHERE date NOT IN (
                SELECT date FROM (
                    SELECT DISTINCT date FROM daily_bar_cache ORDER BY date DESC LIMIT ?
                )
             )",
        )
        .bind(keep_dates as i64)
        .execute(&self.pool)
        .await?;

        // 缓存为空时 MIN(date) 为 NULL，`first_date < NULL` 恒不成立，不会误删
        sqlx::query(
            "DELETE FROM kline_series_meta WHERE first_date < (
                SELECT MIN(date) FROM (
                    SELECT DISTINCT date FROM daily_bar_cache ORDER BY date DESC LIMIT ?
                )
             )",
        )
        .bind(keep_dates as i64)
        .execute(&self.pool)
        .await?;

        Ok(r.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一份最小可用的日报 payload
    ///
    /// 只填目录接口真正会读的 `breadth.covered_count` 与 `boards`；其余字段给
    /// 空值即可——反序列化要求字段齐全，但不关心内容。
    fn sample_payload(date: &str, covered: u32, boards: usize) -> String {
        let boards: Vec<String> = (0..boards)
            .map(|i| {
                format!(
                    r#"{{"name":"行业{i}","change_pct":1.0,"up_count":1,"down_count":1,
                        "flat_count":0,"leader":null,"leader_code":null,"leader_pct":0.0,
                        "amount":null,"member_count":0,"calc_count":0}}"#
                )
            })
            .collect();
        format!(
            r#"{{"date":"{date}","generated_at":0,
                "lhb":{{"stock_count":0,"record_count":0,"net_total":0.0,"records_sum":0.0,
                        "buy_total":0.0,"sell_total":0.0,"net_buy_count":0,"net_sell_count":0,
                        "amount_total":0.0,"entries":[],"top_reasons":[]}},
                "themes":{{"stock_count":0,"tag_total":0,"theme_total":0,"themes":[],"top_tag":null}},
                "boards":[{boards}],
                "breadth":{{"up_count":0,"down_count":0,"flat_count":0,
                            "covered_count":{covered},"total_amount":null,"indices":[]}},
                "methodology":{{"board_method":"t","from_cache":false,"elapsed_ms":0,"notes":[]}},
                "sources":[]}}"#,
            boards = boards.join(",")
        )
    }

    #[tokio::test]
    async fn test_list_sentiment_daily_reads_counts_from_payload() {
        let db = crate::Database::in_memory().await.unwrap();
        let repo = Repository::from_database(&db);

        for (d, covered, boards) in [
            ("2026-06-30", 5581u32, 128usize),
            ("2026-08-31", 5623, 128),
            ("2026-09-01", 5188, 10),
        ] {
            repo.save_sentiment_daily(d, &sample_payload(d, covered, boards))
                .await
                .unwrap();
        }

        let list = repo.list_sentiment_daily(10).await.unwrap();

        // 按日期倒序——目录页要的就是「最近生成的在最上面」
        assert_eq!(
            list.iter().map(|e| e.date.as_str()).collect::<Vec<_>>(),
            vec!["2026-09-01", "2026-08-31", "2026-06-30"]
        );
        assert_eq!((list[0].covered_count, list[0].board_count), (5188, 10));
        assert_eq!((list[2].covered_count, list[2].board_count), (5581, 128));
        assert!(list.iter().all(|e| e.payload_bytes > 0));
        assert!(
            list.iter().all(|e| !e.has_export),
            "仓库层不碰磁盘，has_export 恒为 false，由路由层按文件系统覆盖"
        );
    }

    #[tokio::test]
    async fn test_list_sentiment_daily_survives_corrupt_payload() {
        let db = crate::Database::in_memory().await.unwrap();
        let repo = Repository::from_database(&db);

        repo.save_sentiment_daily("2026-09-01", "{不是合法 JSON")
            .await
            .unwrap();
        repo.save_sentiment_daily("2026-08-31", &sample_payload("2026-08-31", 100, 3))
            .await
            .unwrap();

        let list = repo.list_sentiment_daily(10).await.unwrap();

        // 一条坏数据不该让整个目录不可用：统计值退化为 0，条目仍保留
        assert_eq!(list.len(), 2);
        let bad = list.iter().find(|e| e.date == "2026-09-01").unwrap();
        assert_eq!((bad.covered_count, bad.board_count), (0, 0));
        let good = list.iter().find(|e| e.date == "2026-08-31").unwrap();
        assert_eq!(good.covered_count, 100);
    }

    #[tokio::test]
    async fn test_delete_sentiment_daily_reports_whether_it_removed() {
        let db = crate::Database::in_memory().await.unwrap();
        let repo = Repository::from_database(&db);
        repo.save_sentiment_daily("2026-09-01", &sample_payload("2026-09-01", 1, 1))
            .await
            .unwrap();

        assert!(repo.delete_sentiment_daily("2026-09-01").await.unwrap());
        assert!(
            !repo.delete_sentiment_daily("2026-09-01").await.unwrap(),
            "重复删除应返回 false，调用方据此区分「删掉了」与「本来就没有」"
        );
        assert!(repo.list_sentiment_daily(10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_latest_job_run_returns_most_recent() {
        let db = crate::Database::in_memory().await.unwrap();
        let repo = Repository::from_database(&db);

        assert!(
            repo.latest_job_run("sentiment-archive").await.unwrap().is_none(),
            "没跑过时不应凭空造一条"
        );

        repo.record_job_run("sentiment-archive", "2026-09-01", "succeeded", "已归档", Some(23_000))
            .await
            .unwrap();
        // 同一秒写入两条时靠 id 兜底排序，这里错开时间戳更贴近真实情形
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        repo.record_job_run("sentiment-archive", "2026-09-02", "failed", "上游限流", Some(110_000))
            .await
            .unwrap();

        let last = repo.latest_job_run("sentiment-archive").await.unwrap().unwrap();
        assert_eq!(last.date, "2026-09-02");
        assert!(last.is_failed());
        assert_eq!(last.detail.as_deref(), Some("上游限流"));
        assert_eq!(last.elapsed_ms, Some(110_000));
    }

    #[tokio::test]
    async fn test_latest_job_run_ignores_skipped_runs() {
        // 探测式触发一天会留下几十条「已归档，无需重复生成」。若把它们当最近一次
        // 运行展示，用户就永远看不到归档到底成没成、花了多久。
        let db = crate::Database::in_memory().await.unwrap();
        let repo = Repository::from_database(&db);

        repo.record_job_run("sentiment-archive", "2026-09-03", "succeeded", "已归档：覆盖 5189 只", Some(5_200))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        repo.record_job_run("sentiment-archive", "2026-09-03", "skipped", "该日已归档，无需重复生成", None)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        repo.record_job_run("sentiment-archive", "2026-09-03", "skipped", "用户在页面上正在生成该日日报，定时任务让路", None)
            .await
            .unwrap();

        let last = repo.latest_job_run("sentiment-archive").await.unwrap().unwrap();
        assert!(last.is_ok());
        assert_eq!(last.detail.as_deref(), Some("已归档：覆盖 5189 只"));
        assert_eq!(last.elapsed_ms, Some(5_200));
    }

    #[tokio::test]
    async fn test_latest_job_run_none_when_only_skipped() {
        // 刚开启自动归档、当日行情未落库时会是这种状态：跑过，但都没实际动作
        let db = crate::Database::in_memory().await.unwrap();
        let repo = Repository::from_database(&db);
        repo.record_job_run("sentiment-archive", "2026-09-03", "skipped", "该日已归档", None)
            .await
            .unwrap();

        assert!(repo.latest_job_run("sentiment-archive").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_has_successful_run_ignores_failures_and_other_jobs() {
        let db = crate::Database::in_memory().await.unwrap();
        let repo = Repository::from_database(&db);

        // 只有 succeeded 才算跑过：failed 应该让下一轮继续重试
        repo.record_job_run("prune", "2026-09-02", "failed", "boom", None)
            .await
            .unwrap();
        assert!(!repo.has_successful_run("prune", "2026-09-02").await.unwrap());

        repo.record_job_run("prune", "2026-09-02", "succeeded", "清理 0 行", Some(3))
            .await
            .unwrap();
        assert!(repo.has_successful_run("prune", "2026-09-02").await.unwrap());

        // 任务名是隔离维度：清理任务跑过不代表归档跑过
        assert!(!repo.has_successful_run("sentiment-archive", "2026-09-02").await.unwrap());
        // 日期也是隔离维度
        assert!(!repo.has_successful_run("prune", "2026-09-01").await.unwrap());
    }

    #[tokio::test]
    async fn test_prune_daily_bars_keeps_only_recent_dates() {
        let db = crate::Database::in_memory().await.unwrap();
        let repo = Repository::from_database(&db);

        for date in ["2026-08-01", "2026-08-02", "2026-08-03", "2026-08-04"] {
            repo.save_daily_bars(&[
                ("000001".to_string(), date.to_string(), 1.0, 0.5, 0.0),
                ("000002".to_string(), date.to_string(), 2.0, -0.5, 0.0),
            ])
            .await
            .unwrap();
        }
        assert_eq!(repo.count_daily_bars("2026-08-01").await.unwrap(), 2);

        let removed = repo.prune_daily_bars(2).await.unwrap();
        assert_eq!(removed, 4, "应删掉最早的两个日期共 4 行");

        assert_eq!(repo.count_daily_bars("2026-08-03").await.unwrap(), 2);
        assert_eq!(repo.count_daily_bars("2026-08-04").await.unwrap(), 2);
        assert_eq!(repo.count_daily_bars("2026-08-01").await.unwrap(), 0);
        assert_eq!(repo.count_daily_bars("2026-08-02").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_prune_daily_bars_is_noop_when_under_limit() {
        let db = crate::Database::in_memory().await.unwrap();
        let repo = Repository::from_database(&db);
        repo.save_daily_bars(&[("000001".to_string(), "2026-08-01".to_string(), 1.0, 0.5, 0.0)])
            .await
            .unwrap();

        assert_eq!(repo.prune_daily_bars(250).await.unwrap(), 0);
        assert_eq!(repo.count_daily_bars("2026-08-01").await.unwrap(), 1);
    }

    /// 跨过单条 INSERT 的分块边界（[`BAR_INSERT_CHUNK`]）
    ///
    /// 启用序列缓存后单次写入是几十万行，必须合成多行 INSERT；这个测试保证
    /// 分块拼接本身没错——块内、跨块、不足一块的余数三种情况都覆盖到。
    #[tokio::test]
    async fn test_save_daily_bars_spans_multiple_insert_chunks() {
        let db = crate::Database::in_memory().await.unwrap();
        let repo = Repository::from_database(&db);

        // 2.5 个分块：覆盖「整块 + 跨块 + 余数」
        let n = BAR_INSERT_CHUNK * 2 + BAR_INSERT_CHUNK / 2;
        let rows: Vec<(String, String, f64, f64, f64)> = (0..n)
            .map(|i| (format!("{i:06}"), "2026-08-01".to_string(), i as f64, 1.5, 0.0))
            .collect();
        repo.save_daily_bars(&rows).await.unwrap();

        assert_eq!(repo.count_daily_bars("2026-08-01").await.unwrap(), n as i64);

        // 再写一遍同主键不同值，验证多行 INSERT 的 ON CONFLICT 分支也生效
        let twice: Vec<(String, String, f64, f64, f64)> = (0..n)
            .map(|i| (format!("{i:06}"), "2026-08-01".to_string(), 99.0, 1.5, 0.0))
            .collect();
        repo.save_daily_bars(&twice).await.unwrap();

        let all = repo.get_daily_bars("2026-08-01").await.unwrap();
        assert_eq!(all.len(), n);
        assert!(all.iter().all(|(_, close, _, _)| (*close - 99.0).abs() < 1e-12));
    }

    /// 清理日线时，起点被清掉的序列记录必须一起失效
    ///
    /// 否则 meta 仍声称覆盖了那个日期，上层据此把「数据已被清理」误判成
    /// 「当日停牌」——一个不会报错、只会悄悄算错的错误
    #[tokio::test]
    async fn test_prune_daily_bars_invalidates_stale_series_meta() {
        let db = crate::Database::in_memory().await.unwrap();
        let repo = Repository::from_database(&db);

        // 000001 的序列覆盖 08-01..08-03，000002 只覆盖 08-02..08-03
        for (code, dates) in [
            ("000001", vec!["2026-08-01", "2026-08-02", "2026-08-03"]),
            ("000002", vec!["2026-08-02", "2026-08-03"]),
        ] {
            let rows: Vec<_> = dates
                .iter()
                .map(|d| (code.to_string(), d.to_string(), 1.0, 0.5, 0.0))
                .collect();
            repo.save_daily_bars(&rows).await.unwrap();
            repo.save_series_meta(&[SeriesMetaRow {
                stock_code: code.to_string(),
                fetched_at: "2026-09-04".to_string(),
                first_date: dates[0].to_string(),
                last_date: "2026-08-03".to_string(),
                complete: false,
            }])
            .await
            .unwrap();
        }

        // 只保留最近 2 个日期，08-01 被清掉 → 000001 的 meta 起点失效
        let removed = repo.prune_daily_bars(2).await.unwrap();
        assert_eq!(removed, 1, "应只删掉 08-01 那一行");

        let metas = repo.all_series_meta().await.unwrap();
        assert_eq!(metas.len(), 1, "起点被清掉的序列记录应一并失效");
        assert!(
            !metas.contains_key("000001"),
            "000001 的序列起点 2026-08-01 已被清理，保留它会让上层误判停牌"
        );
        assert!(metas.contains_key("000002"));
    }

    /// 待刷新队列必须是「轮转」的：今天刷过的那批要排到末尾
    ///
    /// 这是滚动刷新能覆盖全市场的唯一依据。若排序不稳（比如同 `fetched_at` 内
    /// 顺序随机），每天取前 N 只就可能重复刷同一批、永远轮不到另一批——而故障
    /// 表现只是「部分股票历史价陈旧」，没有任何报错。
    #[tokio::test]
    async fn test_oldest_series_rotates() {
        let db = crate::Database::in_memory().await.unwrap();
        let repo = Repository::from_database(&db);

        // 三批不同抓取日期；同批次内的代码故意乱序写入，验证二级排序
        for (code, fetched) in [
            ("000003", "2026-09-01"),
            ("000001", "2026-09-01"),
            ("000002", "2026-09-02"),
            ("000005", "2026-09-03"),
            ("000004", "2026-09-03"),
        ] {
            repo.save_series_meta(&[SeriesMetaRow {
                stock_code: code.to_string(),
                fetched_at: fetched.to_string(),
                first_date: "2026-03-01".to_string(),
                last_date: "2026-09-03".to_string(),
                complete: false,
            }])
            .await
            .unwrap();
        }

        assert_eq!(repo.count_series_meta().await.unwrap(), 5);

        // 最旧的一批先出，且批内按代码升序（不是写入顺序）
        let codes: Vec<_> = repo
            .oldest_series(2)
            .await
            .unwrap()
            .into_iter()
            .map(|(c, _)| c)
            .collect();
        assert_eq!(codes, vec!["000001", "000003"], "同批次内应按代码升序");

        // 把它们刷成今天 → 应排到队尾，明天轮到下一批
        let today = "2026-09-04";
        for c in &codes {
            repo.save_series_meta(&[SeriesMetaRow {
                stock_code: c.clone(),
                fetched_at: today.to_string(),
                first_date: "2026-03-01".to_string(),
                last_date: "2026-09-04".to_string(),
                complete: false,
            }])
            .await
            .unwrap();
        }

        let codes: Vec<_> = repo
            .oldest_series(2)
            .await
            .unwrap()
            .into_iter()
            .map(|(c, _)| c)
            .collect();
        assert_eq!(
            codes,
            vec!["000002", "000004"],
            "刷新过的两只要排到末尾，剩下的按抓取日期继续轮转"
        );

        // 一并返回序列起点：刷新时据此决定窗口要放多大
        let firsts: Vec<_> = repo
            .oldest_series(5)
            .await
            .unwrap()
            .into_iter()
            .map(|(_, f)| f)
            .collect();
        assert_eq!(firsts.len(), 5, "limit 超过总数时应全部返回，不报错");
        assert!(firsts.iter().all(|f| f == "2026-03-01"));
    }

    /// 空表不能返回越界或报错：首次冷启动前归档任务就会跑到这条路径
    #[tokio::test]
    async fn test_oldest_series_on_empty_table() {
        let db = crate::Database::in_memory().await.unwrap();
        let repo = Repository::from_database(&db);

        assert_eq!(repo.count_series_meta().await.unwrap(), 0);
        assert!(repo.oldest_series(100).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_series_meta_roundtrip() {
        let db = crate::Database::in_memory().await.unwrap();
        let repo = Repository::from_database(&db);

        assert!(repo.all_series_meta().await.unwrap().is_empty());

        repo.save_series_meta(&[
            SeriesMetaRow {
                stock_code: "600519".to_string(),
                fetched_at: "2026-09-04".to_string(),
                first_date: "2026-03-01".to_string(),
                last_date: "2026-09-03".to_string(),
                complete: false,
            },
            SeriesMetaRow {
                stock_code: "301688".to_string(),
                fetched_at: "2026-09-04".to_string(),
                first_date: "2026-09-02".to_string(),
                last_date: "2026-09-03".to_string(),
                // 次新股：返回根数少于请求根数，已回溯到上市首日
                complete: true,
            },
        ])
        .await
        .unwrap();

        let metas = repo.all_series_meta().await.unwrap();
        assert_eq!(metas.len(), 2);
        assert_eq!(metas["600519"].first_date, "2026-03-01");
        assert!(!metas["600519"].complete);
        assert!(metas["301688"].complete);
        assert_eq!(metas["301688"].fetched_at, "2026-09-04");

        // 再抓一次要覆盖旧记录，而不是并存
        repo.save_series_meta(&[SeriesMetaRow {
            stock_code: "600519".to_string(),
            fetched_at: "2026-09-05".to_string(),
            first_date: "2026-03-02".to_string(),
            last_date: "2026-09-04".to_string(),
            complete: true,
        }])
        .await
        .unwrap();

        let metas = repo.all_series_meta().await.unwrap();
        assert_eq!(metas.len(), 2, "同代码重复写入应是更新而非新增");
        assert_eq!(metas["600519"].fetched_at, "2026-09-05");
        assert_eq!(metas["600519"].first_date, "2026-03-02");
        assert!(metas["600519"].complete);
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
