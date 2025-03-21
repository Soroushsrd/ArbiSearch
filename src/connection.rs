use alloy::providers::{Provider, ProviderBuilder, WsConnect};
use eyre::Result;
use std::sync::Arc;
use tracing::{debug, info, instrument};

use crate::config::ProviderConfig;

pub struct ConnectionManager {
    pub wss_provider: Arc<dyn Provider>,
    pub https_provider: Arc<dyn Provider>,
    pub config: Arc<ProviderConfig>,
}

impl ConnectionManager {
    #[instrument(name = "initialize_connection")]
    pub async fn initialize() -> Result<Self> {
        info!("Initializing connection manager");
        let config = ProviderConfig::from_env();

        debug!(ws_url = %config.ws_url, "Creating WebSocket connection");
        let ws = WsConnect::new(&config.ws_url);
        let wss_provider = ProviderBuilder::new().on_ws(ws).await?;
        info!("WebSocket provider initialized successfully");

        debug!(https_url = %config.https_url, "Creating HTTPS connection");
        let https_provider = ProviderBuilder::new().on_http(config.https_url.parse()?);
        info!("HTTPS provider initialized successfully");

        Ok(Self {
            wss_provider: Arc::new(wss_provider),
            https_provider: Arc::new(https_provider),
            config: Arc::new(config),
        })
    }
}
