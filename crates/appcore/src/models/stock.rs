use serde::{Deserialize, Serialize};

/// 股票市场
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum Market {
    Sh,
    Sz,
    Bj,
}

impl Market {
    /// 根据股票代码前缀推断市场
    pub fn from_code(code: &str) -> Self {
        match code {
            c if c.starts_with("6") | c.starts_with("9") => Market::Sh,
            c if c.starts_with("0") | c.starts_with("2") | c.starts_with("3") => Market::Sz,
            c if c.starts_with("4") | c.starts_with("8") => Market::Bj,
            _ => Market::Sz,
        }
    }

    /// 获取市场前缀代码 (用于东方财富 API)
    pub fn secid_prefix(&self) -> &str {
        match self {
            Market::Sh => "1",
            Market::Sz => "0",
            Market::Bj => "0",
        }
    }
}

/// 股票基础信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stock {
    pub code: String,
    pub name: String,
    pub market: Market,
    pub industry: Option<String>,
    pub list_date: Option<String>,
    pub status: StockStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StockStatus {
    #[default]
    Active,
    Delisted,
    Suspended,
}
