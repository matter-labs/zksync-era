use std::env;

use zksync_basic_types::Address;
use zksync_config::configs::base_token::BaseTokenConfig;

use crate::FromEnv;

impl FromEnv for BaseTokenConfig {
    fn from_env() -> anyhow::Result<Self> {
        let base_token_address = env::var("BASE_TOKEN_BASE_TOKEN_ADDRESS")
            .ok()
            .map(|s| s.parse())
            .transpose()?;

        let outdated_token_price_timeout = env::var("BASE_TOKEN_OUTDATED_TOKEN_PRICE_TIMEOUT")
            .ok()
            .map(|s| s.parse())
            .transpose()?;
        Ok(Self {
            base_token_address: base_token_address.unwrap(),
            outdated_token_price_timeout,
        })
    }
}
