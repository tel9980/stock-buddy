pub mod macd;
pub mod kdj;
pub mod rsi;
pub mod bollinger;
pub mod boll_shape;
pub mod support_resistance;

// Re-export for convenience
pub use macd::{MacdResult, calc_macd};
pub use kdj::{KdjResult, KdjCross, KdjDivergence, calc_kdj, detect_cross, detect_divergence};
pub use rsi::{RsiResult, calc_rsi};
pub use bollinger::{BollResult, calc_boll};
pub use boll_shape::{BollShape, BollShapeResult, calc_boll_shape};
pub use support_resistance::{SupportResistance, PriceLevel, PriceStrength, calc_support_resistance};
