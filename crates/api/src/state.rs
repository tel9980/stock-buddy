use std::sync::Arc;

use analyzer::AiService;

pub struct AppState {
    pub listen_addr: String,
    pub fetcher: Arc<dyn fetcher::DataFetcher>,
    pub db: storage::Database,
    pub ai: Arc<AiService>,
}

impl AppState {
    pub fn new(
        listen_addr: String,
        fetcher: Arc<dyn fetcher::DataFetcher>,
        db: storage::Database,
        ai: AiService,
    ) -> Self {
        Self {
            listen_addr,
            fetcher,
            db,
            ai: Arc::new(ai),
        }
    }
}
