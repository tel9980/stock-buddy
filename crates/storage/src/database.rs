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
        tracing::info!("Database migrations completed");
        Ok(())
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}
