use zksync_config::configs::base_token::BaseTokenConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for BaseTokenConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("base_token", "BASE_TOKEN_")
    }
}
