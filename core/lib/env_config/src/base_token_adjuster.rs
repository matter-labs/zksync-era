use zksync_config::configs::BaseTokenAdjusterConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for BaseTokenAdjusterConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("base_token_adjuster", "BASE_TOKEN_ADJUSTER_")
    }
}
