pub mod traits;
pub mod eastmoney;
pub mod sina;

pub use traits::*;
pub use eastmoney::EastmoneyFetcher;
pub use sina::SinaFetcher;
