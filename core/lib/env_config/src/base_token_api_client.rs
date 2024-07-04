use zksync_config::configs::BaseTokenApiClientConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for BaseTokenApiClientConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("base_token_api_client", "BASE_TOKEN_API_CLIENT_")
    }
}
