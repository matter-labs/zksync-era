use zksync_config::configs::TxSinkConfig;

use crate::FromEnv;

impl FromEnv for TxSinkConfig {
    fn from_env() -> anyhow::Result<Self> {
        let deny_list = std::env::var("DENY_LIST").ok();

        Ok(TxSinkConfig { deny_list })
    }
}
