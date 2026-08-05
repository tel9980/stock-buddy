-- 股票基础信息表
CREATE TABLE IF NOT EXISTS stocks (
    code TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    market TEXT NOT NULL,
    industry TEXT,
    list_date TEXT,
    status TEXT DEFAULT 'active'
);

CREATE INDEX IF NOT EXISTS idx_stocks_industry ON stocks(industry);

-- 日 K 线数据表
CREATE TABLE IF NOT EXISTS kline_daily (
    stock_code TEXT NOT NULL,
    date TEXT NOT NULL,
    open REAL NOT NULL,
    close REAL NOT NULL,
    high REAL NOT NULL,
    low REAL NOT NULL,
    volume REAL NOT NULL,
    amount REAL NOT NULL,
    pct_change REAL,
    turnover REAL,
    PRIMARY KEY (stock_code, date)
);

CREATE INDEX IF NOT EXISTS idx_kline_daily_date ON kline_daily(date);

-- 自选股表
CREATE TABLE IF NOT EXISTS watchlist (
    user_id TEXT NOT NULL DEFAULT 'default',
    stock_code TEXT NOT NULL,
    group_name TEXT DEFAULT 'default',
    note TEXT,
    add_time TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    PRIMARY KEY (user_id, stock_code)
);

-- 告警规则表
CREATE TABLE IF NOT EXISTS alert_rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id TEXT NOT NULL DEFAULT 'default',
    stock_code TEXT NOT NULL,
    rule_type TEXT NOT NULL,
    params TEXT,
    status TEXT DEFAULT 'active',
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_alert_rules_user ON alert_rules(user_id, stock_code);

-- 告警历史表
CREATE TABLE IF NOT EXISTS alert_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id TEXT NOT NULL DEFAULT 'default',
    rule_id INTEGER,
    stock_code TEXT NOT NULL,
    triggered_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    alert_content TEXT NOT NULL,
    status TEXT DEFAULT 'unread'
);

CREATE INDEX IF NOT EXISTS idx_alert_history_user ON alert_history(user_id, triggered_at DESC);

-- AI 分析历史记录表
CREATE TABLE IF NOT EXISTS ai_analysis_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id TEXT NOT NULL DEFAULT 'default',
    analysis_type TEXT NOT NULL,       -- diagnose | predict | market | chat
    title TEXT NOT NULL,              -- 标题 / 标的
    stock_code TEXT,                  -- 个股代码（可选）
    stock_name TEXT,                  -- 个股名称（可选）
    question TEXT,                    -- 智能问答的问题（可选）
    content TEXT NOT NULL,            -- 完整分析内容（Markdown / HTML）
    model TEXT,                       -- 使用的模型
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE INDEX IF NOT EXISTS idx_ai_history_user ON ai_analysis_history(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_ai_history_type ON ai_analysis_history(analysis_type);
