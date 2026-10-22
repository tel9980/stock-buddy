use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use std::path::Path;
use std::str::FromStr;

/// 数据库连接管理
#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// 从文件路径创建数据库
    pub async fn new(db_path: &str) -> Result<Self> {
        // 确保目录存在
        if let Some(parent) = Path::new(db_path).parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

        let pool = SqlitePool::connect_with(options).await?;

        let db = Self { pool };
        db.run_migrations().await?;
        Ok(db)
    }

    /// 内存数据库 (用于测试)
    pub async fn in_memory() -> Result<Self> {
        let pool = SqlitePool::connect("sqlite::memory:").await?;
        let db = Self { pool };
        db.run_migrations().await?;
        Ok(db)
    }

    /// 执行迁移
    async fn run_migrations(&self) -> Result<()> {
        let sql = include_str!("../../../migrations/001_init.sql");
        sqlx::query(sql).execute(&self.pool).await?;

        // 市场情绪日报相关表（stock_industry / daily_bar_cache / sentiment_daily）
        let sql = include_str!("../../../migrations/002_sentiment.sql");
        sqlx::query(sql).execute(&self.pool).await?;

        // 定时任务运行记录（job_runs），供自动归档的可观测
        let sql = include_str!("../../../migrations/003_jobs.sql");
        sqlx::query(sql).execute(&self.pool).await?;

        // 日线序列缓存的抓取记录（kline_series_meta），用于跳过重复抓取
        let sql = include_str!("../../../migrations/004_kline_series.sql");
        sqlx::query(sql).execute(&self.pool).await?;

        // 自选股扩展字段：成本价 / 持股数（列可能已存在，忽略报错）
        sqlx::query(
            "ALTER TABLE watchlist ADD COLUMN cost_price REAL NOT NULL DEFAULT 0",
        )
        .execute(&self.pool)
        .await
        .ok();
        sqlx::query(
            "ALTER TABLE watchlist ADD COLUMN shares INTEGER NOT NULL DEFAULT 0",
        )
        .execute(&self.pool)
        .await
        .ok();

        tracing::info!("Database migrations completed");
        Ok(())
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}
