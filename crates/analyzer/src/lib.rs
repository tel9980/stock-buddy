pub mod ai;
pub mod market;
pub mod diagnosis;
pub mod technical;
pub mod capital;
pub mod signals;
pub mod volume_price;
pub mod tape_reading;
pub mod sector_analysis;
pub mod t_strategy;
pub mod backtest;
pub mod sentiment;

pub use ai::AiService;
pub use market::MarketAnalyzer;
pub use diagnosis::StockDiagnoser;
pub use technical::TechnicalService;
pub use capital::CapitalAnalyzer;
pub use signals::{SignalAnalysis, TradeSignal, SignalType, analyze_signals};
pub use volume_price::{
    VolumePriceAnalysis, VpBar, VpSignal, VpSignalType, VpRelationship, analyze_volume_price,
};
pub use tape_reading::{
    TapeReadingAnalysis, TapeReadingSignal, analyze_tape_reading,
};
pub use sector_analysis::{HotSectorSeed, analyze_sector_hotspot, score_hot_sectors};
pub use t_strategy::analyze_t_signals;
pub use backtest::{run_t_backtest, scan_t_params, build_conclusion};
pub use sentiment::{
    aggregate_boards, build_theme_summary, calc_breadth, summarize_lhb, DailyBar,
};
