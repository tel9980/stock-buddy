use serde::{Deserialize, Serialize};

/// AI 分析类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnalysisType {
    /// 个股诊断
    Diagnose,
    /// 趋势预测
    Predict,
    /// 市场研判
    Market,
    /// 智能问答
    Chat,
}

impl AnalysisType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AnalysisType::Diagnose => "diagnose",
            AnalysisType::Predict => "predict",
            AnalysisType::Market => "market",
            AnalysisType::Chat => "chat",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "diagnose" => Some(AnalysisType::Diagnose),
            "predict" => Some(AnalysisType::Predict),
            "market" => Some(AnalysisType::Market),
            "chat" => Some(AnalysisType::Chat),
            _ => None,
        }
    }

    /// 中文展示名
    pub fn label(&self) -> &'static str {
        match self {
            AnalysisType::Diagnose => "个股诊断",
            AnalysisType::Predict => "趋势预测",
            AnalysisType::Market => "市场研判",
            AnalysisType::Chat => "智能问答",
        }
    }
}

/// AI 分析历史记录（完整）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAnalysisHistory {
    pub id: i64,
    pub user_id: String,
    pub analysis_type: String,
    pub title: String,
    pub stock_code: Option<String>,
    pub stock_name: Option<String>,
    pub question: Option<String>,
    pub content: String,
    pub model: Option<String>,
    pub created_at: String,
}

/// AI 分析历史列表项（不含完整 content，仅摘要）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiHistoryItem {
    pub id: i64,
    pub analysis_type: String,
    pub title: String,
    pub stock_code: Option<String>,
    pub stock_name: Option<String>,
    pub question: Option<String>,
    /// 内容摘要（前若干字符）
    pub summary: String,
    pub model: Option<String>,
    pub created_at: String,
}

/// 新建历史记录请求体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewAiHistory {
    pub analysis_type: String,
    pub title: String,
    pub stock_code: Option<String>,
    pub stock_name: Option<String>,
    pub question: Option<String>,
    pub content: String,
    pub model: Option<String>,
}
