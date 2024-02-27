use zksync_config::configs::native_token_fetcher::NativeTokenFetcherConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for NativeTokenFetcherConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("native_token_fetcher", "NATIVE_TOKEN_FETCHER_")
    }
}
