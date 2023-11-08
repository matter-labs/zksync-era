use zksync_config::ETHClientConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for ETHClientConfig {
    fn from_env() -> anyhow::Result<Self> {
        let config: Self = envy_load("eth_client", "ETH_CLIENT_")?;
        if config.web3_url.find(',').is_some() {
            anyhow::bail!(
                "Multiple web3 URLs aren't supported anymore. Provided invalid value: {}",
                config.web3_url
            );
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> ETHClientConfig {
        ETHClientConfig {
            chain_id: 9,
            web3_url: "http://127.0.0.1:8545".into(),
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            ETH_CLIENT_CHAIN_ID="9"
            ETH_CLIENT_WEB3_URL="http://127.0.0.1:8545"
        "#;
        lock.set_env(config);

        let actual = ETHClientConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
