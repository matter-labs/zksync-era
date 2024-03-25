use zksync_config::configs::base_token_fetcher::BaseTokenFetcherConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for BaseTokenFetcherConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("base_token_fetcher", "base_token_fetcher_")
    }
}
