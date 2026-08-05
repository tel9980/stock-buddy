pub mod macd;
pub mod kdj;
pub mod rsi;
pub mod bollinger;
pub mod support_resistance;

// Re-export for convenience
pub use macd::{MacdResult, calc_macd};
pub use kdj::{KdjResult, calc_kdj};
pub use rsi::{RsiResult, calc_rsi};
pub use bollinger::{BollResult, calc_boll};
pub use support_resistance::{SupportResistance, PriceLevel, PriceStrength, calc_support_resistance};
