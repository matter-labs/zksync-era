use std::{collections::HashMap, time::Duration};

use anyhow::Context as _;
use prometheus_exporter::PrometheusExporterConfig;
use serde::Deserialize;
use vlog::LogFormat;

use super::{ConfigurationSource, Environment};

/// Observability part of the node configuration.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ObservabilityENConfig {
    /// Port to bind the Prometheus exporter server to. If not specified, the server will not be launched.
    /// If the push gateway URL is specified, it will prevail.
    pub prometheus_port: Option<u16>,
    /// Prometheus push gateway to push metrics to. Overrides `prometheus_port`. A full URL must be specified
    /// including `job_id` and other path segments; it will be used verbatim as the URL to push data to.
    pub prometheus_pushgateway_url: Option<String>,
    /// Interval between pushing metrics to the Prometheus push gateway.
    #[serde(default = "ObservabilityENConfig::default_prometheus_push_interval_ms")]
    pub prometheus_push_interval_ms: u64,
    /// Sentry URL to send panics to.
    pub sentry_url: Option<String>,
    /// Environment to use when sending data to Sentry.
    pub sentry_environment: Option<String>,
    /// Log format to use: either `plain` (default) or `json`.
    #[serde(default)]
    pub log_format: LogFormat,
}

impl ObservabilityENConfig {
    const fn default_prometheus_push_interval_ms() -> u64 {
        10_000
    }

    pub fn from_env() -> envy::Result<Self> {
        Self::new(&Environment)
    }

    pub(super) fn new(source: &impl ConfigurationSource) -> envy::Result<Self> {
        const OBSOLETE_VAR_NAMES: &[(&str, &str)] = &[
            ("MISC_SENTRY_URL", "EN_SENTRY_URL"),
            ("MISC_LOG_FORMAT", "EN_LOG_FORMAT"),
        ];

        let en_vars = source.vars().filter_map(|(name, value)| {
            let name = name.into_string().ok()?;
            if !name.starts_with("EN_") {
                return None;
            }
            Some((name, value.into_string().ok()?))
        });
        let mut vars: HashMap<_, _> = en_vars.collect();

        for &(old_name, new_name) in OBSOLETE_VAR_NAMES {
            if vars.contains_key(new_name) {
                continue; // new name is set; it should prevail over the obsolete one.
            }
            if let Some(value) = source.var(old_name) {
                vars.insert(new_name.to_owned(), value);
            }
        }

        envy::prefixed("EN_").from_iter(vars)
    }

    pub fn prometheus(&self) -> Option<PrometheusExporterConfig> {
        match (self.prometheus_port, &self.prometheus_pushgateway_url) {
            (_, Some(url)) => {
                if self.prometheus_port.is_some() {
                    tracing::info!("Both Prometheus port and push gateway URLs are specified; the push gateway URL will be used");
                }
                let push_interval = Duration::from_millis(self.prometheus_push_interval_ms);
                Some(PrometheusExporterConfig::push(url.clone(), push_interval))
            }
            (Some(port), None) => Some(PrometheusExporterConfig::pull(port)),
            (None, None) => None,
        }
    }

    pub fn build_observability(&self) -> anyhow::Result<vlog::ObservabilityGuard> {
        let mut builder = vlog::ObservabilityBuilder::new().with_log_format(self.log_format);
        // Some legacy deployments use `unset` as an equivalent of `None`.
        let sentry_url = self.sentry_url.as_deref().filter(|&url| url != "unset");
        if let Some(sentry_url) = sentry_url {
            builder = builder
                .with_sentry_url(sentry_url)
                .context("Invalid Sentry URL")?
                .with_sentry_environment(self.sentry_environment.clone());
        }
        let guard = builder.build();

        // Report whether sentry is running after the logging subsystem was initialized.
        if let Some(sentry_url) = sentry_url {
            tracing::info!("Sentry configured with URL: {sentry_url}");
        } else {
            tracing::info!("No sentry URL was provided");
        }
        Ok(guard)
    }
}
