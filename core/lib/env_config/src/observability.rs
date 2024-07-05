use zksync_config::configs::{ObservabilityConfig, OpentelemetryConfig};

use crate::FromEnv;

impl FromEnv for ObservabilityConfig {
    fn from_env() -> anyhow::Result<Self> {
        // The logic in this method mimics the historical logic of loading observability options
        // This is left intact, since some of the existing deployments may rely on the this behavior.
        let sentry_url = if let Ok(sentry_url) = std::env::var("MISC_SENTRY_URL") {
            if sentry_url == "unset" {
                None
            } else {
                Some(sentry_url)
            }
        } else {
            None
        };
        let sentry_environment = {
            let l1_network = std::env::var("CHAIN_ETH_NETWORK").ok();
            let l2_network = std::env::var("CHAIN_ETH_ZKSYNC_NETWORK").ok();
            match (l1_network, l2_network) {
                (Some(l1_network), Some(l2_network)) => {
                    Some(format!("{} - {}", l1_network, l2_network))
                }
                _ => None,
            }
        };
        let log_format = if let Ok(log_format) = std::env::var("MISC_LOG_FORMAT") {
            if log_format != "plain" && log_format != "json" {
                anyhow::bail!("MISC_LOG_FORMAT has an unexpected value {}", log_format);
            }
            log_format
        } else {
            "plain".to_string()
        };
        let opentelemetry_level = std::env::var("OPENTELEMETRY_LEVEL").ok();
        let otlp_endpoint = std::env::var("OTLP_ENDPOINT").ok();
        let opentelemetry = match (opentelemetry_level, otlp_endpoint) {
            (Some(level), Some(endpoint)) => Some(OpentelemetryConfig { level, endpoint }),
            _ => None,
        };

        let log_directives = std::env::var("RUST_LOG").ok();

        Ok(ObservabilityConfig {
            sentry_url,
            sentry_environment,
            log_format,
            opentelemetry,
            log_directives,
        })
    }
}
