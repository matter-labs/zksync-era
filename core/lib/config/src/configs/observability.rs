use std::sync::Arc;

use smart_config::{
    fallback::{self, FallbackSource},
    value::{ValueOrigin, WithOrigin},
    DescribeConfig, DeserializeConfig,
};

/// Configuration for the essential observability stack, like logging and sentry integration.
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct ObservabilityConfig {
    #[config(nest)]
    pub sentry: SentryConfig,
    /// Opentelemetry configuration.
    #[config(nest)]
    pub opentelemetry: Option<OpentelemetryConfig>,
    /// Format of the logs as expected by the `vlog` crate.
    /// Currently must be either `plain` or `json`.
    #[config(default_t = "plain".into(), fallback = &fallback::Env("MISC_LOG_FORMAT"))]
    pub log_format: String,
    /// Log directives in format that is used in `RUST_LOG`
    #[config(default_t = "zksync=info".into(), fallback = &fallback::Env("RUST_LOG"))]
    pub log_directives: String,
}

const SENTRY_URL_SOURCE: fallback::Manual =
    fallback::Manual::new("'MISC_SENTRY_URL' env var, unless `unset`", || {
        fallback::Env("MISC_SENTRY_URL")
            .provide_value()
            .filter(|val| val.inner.as_plain_str() != Some("unset"))
    });
const SENTRY_ENV_SOURCE: fallback::Manual =
    fallback::Manual::new("$CHAIN_ETH_NETWORK - $CHAIN_ETH_ZKSYNC_NETWORK", || {
        let l1_network = fallback::Env("CHAIN_ETH_NETWORK").get_raw()?;
        let l2_network = fallback::Env("CHAIN_ETH_ZKSYNC_NETWORK").get_raw()?;
        let origin = Arc::new(ValueOrigin::Synthetic {
            source: Arc::new(ValueOrigin::EnvVars),
            transform: "$CHAIN_ETH_NETWORK - $CHAIN_ETH_ZKSYNC_NETWORK".into(),
        });
        Some(WithOrigin::new(
            format!("{l1_network} - {l2_network}").into(),
            origin,
        ))
    });

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct SentryConfig {
    /// URL of the Sentry instance to send events to.
    #[config(fallback = &SENTRY_URL_SOURCE)]
    pub url: Option<String>,
    /// Name of the environment to use in Sentry.
    #[config(fallback = &SENTRY_ENV_SOURCE)]
    pub environment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct OpentelemetryConfig {
    /// Enables export of span data of specified level (and above) using opentelemetry exporters.
    #[config(default_t = "info".into(), fallback = &fallback::Env("OPENTELEMETRY_LEVEL"))]
    pub level: String,
    /// Opentelemetry HTTP traces collector endpoint.
    #[config(fallback = &fallback::Env("OTLP_ENDPOINT"))]
    pub endpoint: String,
    /// Opentelemetry HTTP logs collector endpoint.
    /// This is optional, since right now the primary way to collect logs is via stdout.
    ///
    /// Important: sending logs via OTLP has only been tested locally, and the performance may be
    /// suboptimal in production environments.
    #[config(fallback = &fallback::Env("OTLP_LOGS_ENDPOINT"))]
    pub logs_endpoint: Option<String>,
}

#[cfg(test)]
mod tests {
    use smart_config::{
        testing::{self, test_complete},
        Yaml,
    };

    use super::*;

    fn expected_config() -> ObservabilityConfig {
        ObservabilityConfig {
            sentry: SentryConfig {
                url: Some("https://sentry.io/".into()),
                environment: Some("goerli - testnet".into()),
            },
            opentelemetry: Some(OpentelemetryConfig {
                level: "info".into(),
                endpoint: "http://otlp-collector/v1/traces".into(),
                logs_endpoint: Some("http://otlp-collector/v1/logs".into()),
            }),
            log_format: "json".into(),
            log_directives: "zksync=info,zksync_state_keeper=debug".into(),
        }
    }

    #[test]
    fn parsing_from_env() {
        let config: ObservabilityConfig = testing::Tester::default()
            .set_env("RUST_LOG", "zksync=info,zksync_state_keeper=debug")
            .set_env("MISC_SENTRY_URL", "https://sentry.io/")
            .set_env("MISC_LOG_FORMAT", "json")
            .set_env("CHAIN_ETH_NETWORK", "goerli")
            .set_env("CHAIN_ETH_ZKSYNC_NETWORK", "testnet")
            .set_env("OPENTELEMETRY_LEVEL", "info")
            .set_env("OTLP_ENDPOINT", "http://otlp-collector/v1/traces")
            .set_env("OTLP_LOGS_ENDPOINT", "http://otlp-collector/v1/logs")
            .test_complete(smart_config::config!())
            .unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
            sentry:
              url: https://sentry.io/
              environment: 'goerli - testnet'
            log_format: json
            opentelemetry:
              level: info
              endpoint: http://otlp-collector/v1/traces
              logs_endpoint: http://otlp-collector/v1/logs
            log_directives: zksync=info,zksync_state_keeper=debug
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: ObservabilityConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
