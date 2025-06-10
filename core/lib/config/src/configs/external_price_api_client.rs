use std::time::Duration;

use smart_config::{DescribeConfig, DeserializeConfig};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct ForcedPriceClientConfig {
    /// Forced conversion ratio
    pub numerator: Option<u64>,
    pub denominator: Option<u64>,
    /// Forced fluctuation. It defines how much percent the ratio should fluctuate from its forced
    /// value. If it's None or 0, then the ForcedPriceClient will return the same quote every time
    /// it's called. Otherwise, ForcedPriceClient will return quote with numerator +/- fluctuation %.
    pub fluctuation: Option<u32>,
    /// In order to smooth out fluctuation, consecutive values returned by forced client will not
    /// differ more than next_value_fluctuation percent.
    #[config(default_t = 3)]
    pub next_value_fluctuation: u32,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct ExternalPriceApiClientConfig {
    #[config(default_t = "noop".into())]
    pub source: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    #[config(default_t = Duration::from_secs(10))]
    pub client_timeout: Duration,
    #[config(nest)]
    pub forced: Option<ForcedPriceClientConfig>,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Environment, Yaml};

    use super::*;

    fn expected_config() -> ExternalPriceApiClientConfig {
        ExternalPriceApiClientConfig {
            source: "no-op".to_string(),
            base_url: Some("https://pro-api.coingecko.com".to_string()),
            api_key: Some("qwerty12345".to_string()),
            client_timeout: Duration::from_secs(10),
            forced: Some(ForcedPriceClientConfig {
                numerator: Some(100),
                denominator: Some(1),
                fluctuation: Some(10),
                next_value_fluctuation: 3,
            }),
        }
    }

    #[test]
    fn parsing_from_env() {
        let env = r#"
            EXTERNAL_PRICE_API_CLIENT_SOURCE=no-op
            EXTERNAL_PRICE_API_CLIENT_BASE_URL=https://pro-api.coingecko.com
            EXTERNAL_PRICE_API_CLIENT_API_KEY=qwerty12345
            EXTERNAL_PRICE_API_CLIENT_CLIENT_TIMEOUT_MS=10000
            EXTERNAL_PRICE_API_CLIENT_FORCED_NUMERATOR=100
            EXTERNAL_PRICE_API_CLIENT_FORCED_DENOMINATOR=1
            EXTERNAL_PRICE_API_CLIENT_FORCED_FLUCTUATION=10
            EXTERNAL_PRICE_API_CLIENT_FORCED_NEXT_VALUE_FLUCTUATION=3
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("EXTERNAL_PRICE_API_CLIENT_");

        let config: ExternalPriceApiClientConfig = test_complete(env).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          source: no-op
          base_url: "https://pro-api.coingecko.com"
          api_key: "qwerty12345"
          client_timeout_ms: 10000
          forced_numerator: 100
          forced_denominator: 1
          forced_next_value_fluctuation: 3
          forced:
            fluctuation: 10
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: ExternalPriceApiClientConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
