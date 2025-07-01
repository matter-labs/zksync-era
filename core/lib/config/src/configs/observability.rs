use std::sync::Arc;

use serde::{Deserialize, Serialize};
use smart_config::{
    de::{Serde, WellKnown},
    fallback,
    value::{ValueOrigin, WithOrigin},
    DescribeConfig, DeserializeConfig,
};

/// Specifies the format of the logs in stdout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    #[default]
    Plain,
    Json,
}

impl WellKnown for LogFormat {
    type Deserializer = Serde![str];
    const DE: Self::Deserializer = Serde![str];
}

/// Configuration for the essential observability stack, like logging and sentry integration.
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct ObservabilityConfig {
    /// Sentry configuration.
    #[config(nest)]
    pub sentry: SentryConfig,
    /// Opentelemetry configuration.
    #[config(nest)]
    pub opentelemetry: Option<OpentelemetryConfig>,
    /// Format of the logs emitted by the node.
    #[config(default, fallback = &fallback::Env("MISC_LOG_FORMAT"))]
    pub log_format: LogFormat,
    /// Log directives in format that is used in `RUST_LOG` by `log` or `tracing` façades.
    #[config(default_t = "zksync=info".into(), fallback = &fallback::Env("RUST_LOG"))]
    pub log_directives: String,
}

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
    #[config(deserialize_if(not_unset, "not empty or 'unset'"), fallback = &fallback::Env("MISC_SENTRY_URL"))]
    pub url: Option<String>,
    /// Name of the environment to use in Sentry.
    #[config(fallback = &SENTRY_ENV_SOURCE)]
    pub environment: Option<String>,
}

fn not_unset(s: &String) -> bool {
    !s.is_empty() && s != "unset"
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
            log_format: LogFormat::Json,
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
