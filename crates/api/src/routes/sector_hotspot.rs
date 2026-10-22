use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use futures::future;
use serde::Serialize;

use crate::state::AppState;
use appcore::{DragonTigerEntry, FinancialSummary, SectorHotspotAnalysis};

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

fn err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    let msg = msg.into();
    tracing::error!(%msg, "请求失败");
    (status, Json(ErrorResponse { error: msg }))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/v1/sector/hotspot", get(sector_hotspot))
}

/// GET /api/v1/sector/hotspot?top=12&include_fundamental=true
///
/// 市场热点板块：聚合行业/概念板块排行 + 板块资金流 + 龙虎榜，
/// 再并发抓取领涨股基本面，给出技术面 + 基本面 + 催化剂的上涨原因分析。
async fn sector_hotspot(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ShParams>,
) -> Result<Json<SectorHotspotAnalysis>, (StatusCode, Json<ErrorResponse>)> {
    let top_n = params.top.unwrap_or(12).clamp(1, 30);
    let include_fundamental = params.include_fundamental.unwrap_or(true);

    // 1) 并行拉取板块排行、板块资金流、龙虎榜
    let (industry, concept, flows, dt) = tokio::join!(
        state.fetcher.fetch_sector_ranks("industry"),
        state.fetcher.fetch_sector_ranks("concept"),
        state.fetcher.fetch_sector_capital_flows(),
        state.fetcher.fetch_dragon_tiger(None),
    );

    let industry = industry.map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let concept = concept.map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let flows = flows.map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let dt_list: Vec<DragonTigerEntry> = dt.unwrap_or_default();

    // 2) 评分并取前 top_n（决定要抓取哪些领涨股基本面）
    let seeds = analyzer::score_hot_sectors(&industry, &concept, &flows, top_n);

    // 3) 并发抓取领涨股财务基本面
    //
    // 按 **代码** 去重与抓取：`leader_stock` 装的是名称（东财 f128），拿它去查
    // `SECURITY_CODE="芒果超媒"` 必然 0 条。代码来自 f140。
    let uniq_leaders: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        seeds
            .iter()
            .filter_map(|s| s.leader_code.clone())
            .filter(|c| seen.insert(c.clone()))
            .collect()
    };

    let financials: HashMap<String, FinancialSummary> = if include_fundamental {
        let fetches = uniq_leaders.into_iter().map(|code| {
            let fetcher = state.fetcher.clone();
            async move {
                match fetcher.fetch_financial_indicators(&code).await {
                    Ok(fin) => Some((code, fin)),
                    Err(e) => {
                        tracing::warn!(%code, %e, "领涨股基本面获取失败");
                        None
                    }
                }
            }
        });
        let results = future::join_all(fetches).await;
        results.into_iter().flatten().collect()
    } else {
        HashMap::new()
    };

    let analysis = analyzer::analyze_sector_hotspot(
        &seeds,
        &financials,
        &dt_list,
        industry.len(),
        concept.len(),
    );
    Ok(Json(analysis))
}

#[derive(serde::Deserialize)]
struct ShParams {
    top: Option<usize>,
    include_fundamental: Option<bool>,
}
