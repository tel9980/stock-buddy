-- 003: 定时任务运行记录
-- 自动归档是无人值守的，跑没跑、跑了多久、成没成，必须能在界面上看得到，
-- 否则故障只会静静地躺在日志文件里。

CREATE TABLE IF NOT EXISTS job_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job TEXT NOT NULL,              -- 任务名，如 sentiment-archive
    date TEXT NOT NULL,             -- 该任务处理的交易日
    outcome TEXT NOT NULL,          -- succeeded | skipped | failed
    detail TEXT,                    -- 一句话说明
    elapsed_ms INTEGER,
    started_at INTEGER NOT NULL
);

-- 前端按「某个任务最近一次运行」查询，job + started_at 倒序即可命中
CREATE INDEX IF NOT EXISTS idx_job_runs_job_started ON job_runs(job, started_at DESC);
