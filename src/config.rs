use dotenv::dotenv;
use serde::{Deserialize, Serialize};
use std::env;
use tracing::{debug, info, instrument};

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub ws_url: String,
    pub https_url: String,
}

impl ProviderConfig {
    #[instrument(name = "load_config")]
    pub fn from_env() -> Self {
        info!("Loading configuration from environment");
        dotenv().ok();
        debug!(".env file loaded successfully");

        Self {
            ws_url: env::var("INFURA_WSS").unwrap(),
            https_url: env::var("INFURA_HTTPS").unwrap(),
        }
    }
}
