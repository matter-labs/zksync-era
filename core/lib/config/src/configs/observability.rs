use smart_config::{DescribeConfig, DeserializeConfig};

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
    #[config(default_t = "plain".into())]
    pub log_format: String,
    /// Log directives in format that is used in `RUST_LOG`
    #[config(default_t = "zksync=info".into())]
    pub log_directives: String,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct SentryConfig {
    /// URL of the Sentry instance to send events to.
    pub url: String,
    /// Name of the environment to use in Sentry.
    pub environment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct OpentelemetryConfig {
    /// Enables export of span data of specified level (and above) using opentelemetry exporters.
    #[config(default_t = "info".into())]
    pub level: String,
    /// Opentelemetry HTTP traces collector endpoint.
    pub endpoint: String,
    /// Opentelemetry HTTP logs collector endpoint.
    /// This is optional, since right now the primary way to collect logs is via stdout.
    ///
    /// Important: sending logs via OTLP has only been tested locally, and the performance may be
    /// suboptimal in production environments.
    pub logs_endpoint: Option<String>,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Yaml};

    use super::*;

    // FIXME: parsing from env is completely arbitrary.

    fn expected_config() -> ObservabilityConfig {
        ObservabilityConfig {
            sentry: Some(SentryConfig {
                url: "https://sentry.io/".into(),
                environment: Some("test".into()),
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
    fn parsing_from_yaml() {
        let yaml = r#"
            sentry:
              url: https://sentry.io/
              environment: test
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
