use zksync_config::configs::{
    external_price_api_client::ForcedPriceClientConfig, ExternalPriceApiClientConfig,
};

use crate::{envy_load, FromEnv};

impl FromEnv for ExternalPriceApiClientConfig {
    fn from_env() -> anyhow::Result<Self> {
        let mut config: ExternalPriceApiClientConfig =
            envy_load("external_price_api_client", "EXTERNAL_PRICE_API_CLIENT_")?;
        config.forced = ForcedPriceClientConfig::from_env().ok();
        Ok(config)
    }
}

impl FromEnv for ForcedPriceClientConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load(
            "external_price_api_client_forced",
            "EXTERNAL_PRICE_API_CLIENT_FORCED_",
        )
    }
}

#[cfg(test)]
mod tests {
    use zksync_config::configs::external_price_api_client::{
        ExternalPriceApiClientConfig, ForcedPriceClientConfig, DEFAULT_TIMEOUT_MS,
    };

    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_external_price_api_client_config() -> ExternalPriceApiClientConfig {
        ExternalPriceApiClientConfig {
            source: "no-op".to_string(),
            base_url: Some("https://pro-api.coingecko.com".to_string()),
            api_key: Some("qwerty12345".to_string()),
            client_timeout_ms: DEFAULT_TIMEOUT_MS,
            forced: Some(ForcedPriceClientConfig {
                numerator: Some(100),
                denominator: Some(1),
                fluctuation: Some(10),
                next_value_fluctuation: 2,
            }),
        }
    }

    #[test]
    fn from_env_external_price_api_client() {
        let mut lock = MUTEX.lock();
        let config = r#"
            EXTERNAL_PRICE_API_CLIENT_SOURCE=no-op
            EXTERNAL_PRICE_API_CLIENT_BASE_URL=https://pro-api.coingecko.com
            EXTERNAL_PRICE_API_CLIENT_API_KEY=qwerty12345
            EXTERNAL_PRICE_API_CLIENT_FORCED_NUMERATOR=100
            EXTERNAL_PRICE_API_CLIENT_FORCED_DENOMINATOR=1
            EXTERNAL_PRICE_API_CLIENT_FORCED_FLUCTUATION=10
        "#;
        lock.set_env(config);

        let actual = ExternalPriceApiClientConfig::from_env().unwrap();
        assert_eq!(actual, expected_external_price_api_client_config());
    }
}
