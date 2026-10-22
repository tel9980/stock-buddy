use serde::{Deserialize, Serialize};

/// 一次定时任务运行记录
///
/// 自动归档是无人值守的：跑没跑、跑成什么样，只有查这张表才知道。前端归档
/// 抽屉顶部展示的「上次自动归档」就来自它。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRunInfo {
    /// 任务名，如 `sentiment-archive`
    pub job: String,
    /// 该任务处理的交易日
    pub date: String,
    /// `succeeded` | `skipped` | `failed`
    pub outcome: String,
    /// 一句话说明；失败时这里是排障的第一手线索，前端会直接展示
    pub detail: Option<String>,
    pub elapsed_ms: Option<u64>,
    pub started_at: i64,
}

impl JobRunInfo {
    pub fn is_ok(&self) -> bool {
        self.outcome == "succeeded"
    }

    pub fn is_failed(&self) -> bool {
        self.outcome == "failed"
    }
}
