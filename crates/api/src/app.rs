use std::sync::Arc;
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::routes;
use crate::state::AppState;

pub struct App {
    pub router: Router,
    pub state: Arc<AppState>,
}

pub async fn create_app() -> anyhow::Result<App> {
    let listen_addr = std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "data/stock.db".to_string());
    let report_dir = std::path::PathBuf::from(
        std::env::var("REPORT_DIR").unwrap_or_else(|_| "data/reports".to_string()),
    );

    // 导出件留存的落盘目录。建不出来不该让服务起不来——下面写文件时也会各自容错，
    // 所以这里只记日志：最坏情况是「归档列表里 has_export 恒为 false」，功能降级而非不可用。
    if let Err(e) = std::fs::create_dir_all(&report_dir) {
        tracing::warn!(dir = %report_dir.display(), error = %e, "导出件目录创建失败，服务端留存将不可用");
    }

    let db = storage::Database::new(&db_path).await?;
    let fetcher = Arc::new(fetcher::EastmoneyFetcher::new());

    // AI 配置：优先读 data/ai-config.json（页面保存的），其次环境变量。
    // 加载后立刻用它构造 LLM 客户端，保证首启就能用页面里存过的 key。
    let data_dir = std::path::Path::new(&db_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("data"));
    let providers = Arc::new(crate::providers::ProviderStore::load(&data_dir));
    let ai_config = Arc::new(crate::ai_config::AiConfigStore::load(&data_dir));
    let ai_settings = ai_config.get();
    let ai_service = analyzer::AiService::new(
        analyzer::ai::LlmClient::new(analyzer::ai::LlmConfig {
            api_key: ai_settings.api_key,
            base_url: ai_settings.base_url,
            model: ai_settings.model,
            ..Default::default()
        }),
        fetcher.clone(),
    );
    if ai_config.is_configured() {
        tracing::info!("AI 已配置（来源: {}）", data_dir.join("ai-config.json").display());
    } else {
        tracing::info!("AI 未配置，可在页面上设置 API Key");
    }
    {
        let st = providers.status();
        tracing::info!(
            llm = st.llm,
            mx = st.mx,
            fuyao = st.fuyao,
            "数据源配置：{}",
            data_dir.join("providers.json").display()
        );
    }

    let state = Arc::new(AppState::new(
        listen_addr,
        fetcher.clone(),
        db,
        ai_service,
        report_dir,
        ai_config,
        providers,
    ));

    // 收盘后自动归档。默认关闭：它会在后台周期性访问行情源，不该在没人知道的
    // 情况下自作主张跑起来。
    if env_flag("AUTO_ARCHIVE") {
        spawn_scheduler(&state);
    } else {
        tracing::info!("自动归档未开启（设 AUTO_ARCHIVE=1 启用）");
    }

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let router = Router::new()
        .merge(routes::quotes::router())
        .merge(routes::kline::router())
        .merge(routes::market::router())
        .merge(routes::watchlist::router())
        .merge(routes::ai::router())
        .merge(routes::providers::router())
        .merge(routes::analysis::router())
        .merge(routes::capital::router())
        .merge(routes::limitup::router())
        .merge(routes::dragontiger::router())
        .merge(routes::finance::router())
        .merge(routes::ai_history::router())
        .merge(routes::volume_price::router())
        .merge(routes::tape_reading::router())
        .merge(routes::sector_hotspot::router())
        .merge(routes::t_backtest::router())
        .merge(routes::sentiment::router())
        .merge(routes::ws::router())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    Ok(App {
        router,
        state,
    })
}

/// 启动调度循环
///
/// 任务体在 `jobs` 模块，这里只负责装配与配置。返回的 JoinHandle 直接丢弃：
/// tokio 的 task 不会因为 handle 被 drop 而取消。
fn spawn_scheduler(state: &Arc<AppState>) {
    let cfg = scheduler::SchedulerConfig {
        interval: std::time::Duration::from_secs(env_parse(
            "AUTO_ARCHIVE_INTERVAL_SECS",
            15 * 60,
        )),
        catchup_days: env_parse("AUTO_ARCHIVE_CATCHUP_DAYS", 3),
        catchup_gap: std::time::Duration::from_secs(env_parse("AUTO_ARCHIVE_CATCHUP_GAP_SECS", 2)),
    };

    tracing::info!(
        interval_secs = cfg.interval.as_secs(),
        catchup_days = cfg.catchup_days,
        "自动归档已开启"
    );

    let probe = scheduler::EmTradeDateProbe::new(fetcher::EastmoneyFetcher::new());
    let mut sched = scheduler::Scheduler::new(cfg, Arc::new(probe)).with_db(state.db.clone());
    sched.register(crate::jobs::sentiment_archive_job(state.clone()));
    sched.register(crate::jobs::cache_prune_job(state.clone()));
    sched.register(crate::jobs::series_refresh_job(state.clone()));

    Arc::new(sched).start();
}

/// 读取布尔型环境变量，1 / true / on / yes 均视为开启
fn env_flag(key: &str) -> bool {
    matches!(
        std::env::var(key)
            .ok()
            .map(|v| v.trim().to_ascii_lowercase())
            .as_deref(),
        Some("1") | Some("true") | Some("on") | Some("yes")
    )
}

/// 读取数值型环境变量，缺失或解析失败时回落到默认值
fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}
