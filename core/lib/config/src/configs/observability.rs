use std::sync::Arc;

use smart_config::{
    alt::{self, AltSource},
    value::{ValueOrigin, WithOrigin},
    DescribeConfig, DeserializeConfig,
};

/// Configuration for the essential observability stack, like logging and sentry integration.
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct ObservabilityConfig {
    #[config(nest)]
    pub sentry: Option<SentryConfig>,
    /// Opentelemetry configuration.
    #[config(nest)]
    pub opentelemetry: Option<OpentelemetryConfig>,
    /// Format of the logs as expected by the `vlog` crate.
    /// Currently must be either `plain` or `json`.
    #[config(default_t = "plain".into(), alt = &alt::Env("MISC_LOG_FORMAT"))]
    pub log_format: String,
    /// Log directives in format that is used in `RUST_LOG`
    #[config(default_t = "zksync=info".into(), alt = &alt::Env("RUST_LOG"))]
    pub log_directives: String,
}

const SENTRY_URL_SOURCE: alt::Custom =
    alt::Custom::new("'MISC_SENTRY_URL' env var, unless `unset`", || {
        alt::Env("MISC_SENTRY_URL")
            .provide_value()
            .filter(|val| val.inner.as_plain_str() != Some("unset"))
    });
const SENTRY_ENV_SOURCE: alt::Custom =
    alt::Custom::new("$CHAIN_ETH_NETWORK - $CHAIN_ETH_ZKSYNC_NETWORK", || {
        let l1_network = alt::Env("CHAIN_ETH_NETWORK").get_raw()?;
        let l2_network = alt::Env("CHAIN_ETH_ZKSYNC_NETWORK").get_raw()?;
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
pub struct SentryConfig {
    /// URL of the Sentry instance to send events to.
    #[config(alt = &SENTRY_URL_SOURCE)]
    pub url: String,
    /// Name of the environment to use in Sentry.
    #[config(alt = &SENTRY_ENV_SOURCE)]
    pub environment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct OpentelemetryConfig {
    /// Enables export of span data of specified level (and above) using opentelemetry exporters.
    #[config(default_t = "info".into(), alt = &alt::Env("OPENTELEMETRY_LEVEL"))]
    pub level: String,
    /// Opentelemetry HTTP traces collector endpoint.
    #[config(alt = &alt::Env("OTLP_ENDPOINT"))]
    pub endpoint: String,
    /// Opentelemetry HTTP logs collector endpoint.
    /// This is optional, since right now the primary way to collect logs is via stdout.
    ///
    /// Important: sending logs via OTLP has only been tested locally, and the performance may be
    /// suboptimal in production environments.
    #[config(alt = &alt::Env("OTLP_LOGS_ENDPOINT"))]
    pub logs_endpoint: Option<String>,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Yaml};

    use super::*;

    fn expected_config() -> ObservabilityConfig {
        ObservabilityConfig {
            sentry: Some(SentryConfig {
                url: "https://sentry.io/".into(),
                environment: Some("goerli - testnet".into()),
            }),
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
        let _guard = alt::MockEnvGuard::new([
            ("RUST_LOG", "zksync=info,zksync_state_keeper=debug"),
            ("MISC_SENTRY_URL", "https://sentry.io/"),
            ("MISC_LOG_FORMAT", "json"),
            ("CHAIN_ETH_NETWORK", "goerli"),
            ("CHAIN_ETH_ZKSYNC_NETWORK", "testnet"),
            ("OPENTELEMETRY_LEVEL", "info"),
            ("OTLP_ENDPOINT", "http://otlp-collector/v1/traces"),
            ("OTLP_LOGS_ENDPOINT", "http://otlp-collector/v1/logs"),
        ]);
        let config: ObservabilityConfig = test_complete(smart_config::config!()).unwrap();
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
