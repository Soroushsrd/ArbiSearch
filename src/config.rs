use dotenv::dotenv;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use tracing::{debug, info, instrument};

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct DexProtocol {
    pub name: String,
    pub factory_addr: String,
    pub router_addr: String,
    pub pools: Vec<String>,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub ws_url: String,
    pub https_url: String,
    pub dex_protocols: HashMap<String, DexProtocol>,
}

impl ProviderConfig {
    #[instrument(name = "load_config")]
    pub fn from_env() -> Self {
        info!("Loading configuration from environment");
        dotenv().ok();
        debug!(".env file loaded successfully");

        let mut dex_protocols = HashMap::new();
        dex_protocols.insert(
            "uniswap_v2".to_string(),
            DexProtocol {
                name: "uniswap_v2".to_string(),
                factory_addr: "0x5C69bEe701ef814a2B6a3EDD4B1652CB9cc5aA6f".to_string(),
                router_addr: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D".to_string(),
                pools: vec![
                    "0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc".to_string(), // USDC/ETH
                    "0x0d4a11d5EEaaC28EC3F61d100daF4d40471f1852".to_string(), // ETH/USDT
                ],
            },
        );
        dex_protocols.insert(
            "uniswap_v3".to_string(),
            DexProtocol {
                name: "Uniswap V3".to_string(),
                factory_addr: "0x1F98431c8aD98523631AE4a59f267346ea31F984".to_string(),
                router_addr: "0xE592427A0AEce92De3Edee1F18E0157C05861564".to_string(),
                pools: vec![
                    "0x8ad599c3A0ff1De082011EFDDc58f1908eb6e6D8".to_string(), // USDC/ETH 0.3%
                    "0x7BeA39867e4169DBe237d55C8242a8f2fcDcc387".to_string(), // ETH/USDT 0.3%
                ],
            },
        );
        dex_protocols.insert(
            "sushiswap".to_string(),
            DexProtocol {
                name: "SushiSwap".to_string(),
                factory_addr: "0xC0AEe478e3658e2610c5F7A4A2E1777cE9e4f2Ac".to_string(),
                router_addr: "0xd9e1cE17f2641f24aE83637ab66a2cca9C378B9F".to_string(),
                pools: vec![
                    "0x397FF1542f962076d0BFE58eA045FfA2d347ACa0".to_string(), // ETH/USDC
                    "0x06da0fd433C1A5d7a4faa01111c044910A184553".to_string(), // ETH/USDT
                ],
            },
        );
        Self {
            ws_url: env::var("INFURA_WSS").unwrap(),
            https_url: env::var("INFURA_HTTPS").unwrap(),
            dex_protocols,
        }
    }
}
