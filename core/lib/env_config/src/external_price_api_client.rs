use zksync_config::configs::ExternalPriceApiClientConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for ExternalPriceApiClientConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("external_price_api_client", "EXTERNAL_PRICE_API_CLIENT_")
    }
}

#[cfg(test)]
mod tests {
    use zksync_config::configs::external_price_api_client::{
        ExternalPriceApiClientConfig, DEFAULT_TIMEOUT_MS,
    };

    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_external_price_api_client_config(api_key: String) -> ExternalPriceApiClientConfig {
        ExternalPriceApiClientConfig {
            source: "no-op".to_string(),
            base_url: Some("https://pro-api.coingecko.com".to_string()),
            api_key: Some(api_key),
            client_timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }

    #[test]
    fn from_env_external_price_api_client() {
        let mut lock = MUTEX.lock();
        let config = r#"
            EXTERNAL_PRICE_API_CLIENT_SOURCE=no-op
            EXTERNAL_PRICE_API_CLIENT_BASE_URL=https://pro-api.coingecko.com
            EXTERNAL_PRICE_API_CLIENT_API_KEY=qwerty12345
        "#;
        lock.set_env(config);

        let api_key = "qwerty12345".to_string();

        let actual = ExternalPriceApiClientConfig::from_env().unwrap();
        assert_eq!(actual, expected_external_price_api_client_config(api_key));
    }
}
