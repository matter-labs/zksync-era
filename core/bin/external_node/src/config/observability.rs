use zksync_config::configs::ObservabilityConfig;

pub fn observability_config_from_env() -> anyhow::Result<ObservabilityConfig> {
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
    let sentry_environment = std::env::var("EN_SENTRY_ENVIRONMENT").ok().or_else(|| {
        let l1_network = std::env::var("CHAIN_ETH_NETWORK").ok();
        let l2_network = std::env::var("CHAIN_ETH_ZKSYNC_NETWORK").ok();
        match (l1_network, l2_network) {
            (Some(l1_network), Some(l2_network)) => {
                Some(format!("{} - {}", l1_network, l2_network))
            }
            _ => None,
        }
    });
    let log_format = if let Ok(log_format) = std::env::var("MISC_LOG_FORMAT") {
        if log_format != "plain" && log_format != "json" {
            anyhow::bail!("MISC_LOG_FORMAT has an unexpected value {}", log_format);
        }
        log_format
    } else {
        "plain".to_string()
    };
    let log_directives = std::env::var("RUST_LOG").ok();

    Ok(ObservabilityConfig {
        sentry_url,
        sentry_environment,
        log_format,
        opentelemetry: None,
        sporadic_crypto_errors_substrs: vec![],
        log_directives,
    })
}
