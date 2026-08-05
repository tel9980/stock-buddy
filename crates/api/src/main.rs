pub mod app;
pub mod routes;
pub mod state;

use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // 创建日志目录
    let log_dir = std::path::Path::new("logs");
    if !log_dir.exists() {
        std::fs::create_dir_all(log_dir)?;
    }

    // 文件日志 appender（按天滚动）
    let file_appender = tracing_appender::rolling::daily(&log_dir, "stock-assister.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // 日志过滤级别
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info,stock_assister=debug".into());

    // 构建订阅器：stdout + 文件双写
    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            // stdout 层：简洁格式 + 时间
            tracing_subscriber::fmt::layer()
                .with_span_events(FmtSpan::CLOSE)
                .with_target(false),
        )
        .with(
            // 文件层：带模块路径和时间戳
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_target(true)
                .with_span_events(FmtSpan::CLOSE),
        )
        .init();

    tracing::info!("日志系统已启动，日志文件目录: {}", log_dir.display());

    let app = app::create_app().await?;
    let addr = app.state.listen_addr.clone();

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Stock-Assister 服务启动于 http://{}", addr);

    axum::serve(listener, app.router).await?;

    Ok(())
}
