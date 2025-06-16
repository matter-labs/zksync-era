//! Extensions for the `ObservabilityConfig` to install the observability stack.

use zksync_vlog::prometheus::PrometheusExporterConfig;

pub(crate) use self::metrics::METRICS;
use crate::configs::{observability::LogFormat, ObservabilityConfig, PrometheusConfig};

mod metrics;

impl ObservabilityConfig {
    /// Installs the observability stack based on the configuration.
    pub fn install(self) -> anyhow::Result<zksync_vlog::ObservabilityGuard> {
        self.install_with_logs(std::convert::identity)
    }

    pub fn install_with_logs(
        self,
        logs_transform: impl FnOnce(zksync_vlog::Logs) -> zksync_vlog::Logs,
    ) -> anyhow::Result<zksync_vlog::ObservabilityGuard> {
        let logs = logs_transform(self.clone().into());
        let sentry = Option::<zksync_vlog::Sentry>::try_from(self.clone())?;
        let opentelemetry = Option::<zksync_vlog::OpenTelemetry>::try_from(self.clone())?;

        let guard = zksync_vlog::ObservabilityBuilder::new()
            .with_logs(Some(logs))
            .with_sentry(sentry)
            .with_opentelemetry(opentelemetry)
            .build();
        tracing::info!("Installed observability stack with the following configuration: {self:?}");
        Ok(guard)
    }
}

impl From<LogFormat> for zksync_vlog::logs::LogFormat {
    fn from(format: LogFormat) -> Self {
        match format {
            LogFormat::Plain => Self::Plain,
            LogFormat::Json => Self::Json,
        }
    }
}

impl From<ObservabilityConfig> for zksync_vlog::Logs {
    fn from(config: ObservabilityConfig) -> Self {
        zksync_vlog::Logs::new(config.log_format.into())
            .with_log_directives(Some(config.log_directives))
    }
}

impl TryFrom<ObservabilityConfig> for Option<zksync_vlog::Sentry> {
    type Error = anyhow::Error;

    fn try_from(config: ObservabilityConfig) -> Result<Self, Self::Error> {
        let Some(sentry_config) = config.sentry else {
            return Ok(None);
        };
        let sentry = zksync_vlog::Sentry::new(&sentry_config.url)?;
        Ok(Some(sentry.with_environment(sentry_config.environment)))
    }
}

impl TryFrom<ObservabilityConfig> for Option<zksync_vlog::OpenTelemetry> {
    type Error = anyhow::Error;

    fn try_from(config: ObservabilityConfig) -> Result<Self, Self::Error> {
        Ok(config
            .opentelemetry
            .map(|config| {
                zksync_vlog::OpenTelemetry::new(
                    &config.level,
                    Some(config.endpoint),
                    config.logs_endpoint,
                )
            })
            .transpose()?)
    }
}

impl PrometheusConfig {
    /// Converts this config to the config for Prometheus exporter. Returns `None` if Prometheus is not configured.
    pub fn to_exporter_config(&self) -> Option<PrometheusExporterConfig> {
        if let Some(base_url) = &self.pushgateway_url {
            let gateway_endpoint = PrometheusExporterConfig::gateway_endpoint(base_url);
            Some(PrometheusExporterConfig::push(
                gateway_endpoint,
                self.push_interval(),
            ))
        } else {
            self.to_pull_config()
        }
    }

    /// A version of [`Self::into_exporter_config()`] that only ever creates a pull exporter.
    pub fn to_pull_config(&self) -> Option<PrometheusExporterConfig> {
        self.listener_port.map(PrometheusExporterConfig::pull)
    }
}
