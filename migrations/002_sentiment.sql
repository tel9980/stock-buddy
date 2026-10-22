-- 002: 市场情绪日报（sentiment）相关表
-- 金额单位统一为「万元」，与项目其余模型保持一致。

-- 个股 → 行业归属（东财二级行业口径，与东财行业板块清单名称对齐）
-- 行业归属会随公司业务变化而调整，故记录更新时间，便于定期刷新。
CREATE TABLE IF NOT EXISTS stock_industry (
    code TEXT PRIMARY KEY,
    name TEXT,
    industry TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_stock_industry_industry ON stock_industry(industry);

-- 个股日线缓存
-- 历史交易日的行业涨跌幅无法从官方接口直接取得，需按成分股日线等权聚合。
-- 逐只抓取耗时较长（数千只），落库缓存后同一交易日重复查询可秒级返回。
CREATE TABLE IF NOT EXISTS daily_bar_cache (
    stock_code TEXT NOT NULL,
    date TEXT NOT NULL,
    close REAL NOT NULL,
    pct_change REAL NOT NULL,
    amount REAL NOT NULL,
    PRIMARY KEY (stock_code, date)
);

CREATE INDEX IF NOT EXISTS idx_daily_bar_cache_date ON daily_bar_cache(date);

-- 市场情绪日报整份缓存（JSON 序列化后的完整报告）
CREATE TABLE IF NOT EXISTS sentiment_daily (
    date TEXT PRIMARY KEY,
    payload TEXT NOT NULL,
    generated_at INTEGER NOT NULL
);
