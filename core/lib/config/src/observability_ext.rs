//! Extensions for the `ObservabilityConfig` to install the observability stack.

use crate::configs::ObservabilityConfig;

impl ObservabilityConfig {
    /// Installs the observability stack based on the configuration.
    ///
    /// If any overrides are needed, consider using the `TryFrom` implementations.
    pub fn install(self) -> anyhow::Result<zksync_vlog::ObservabilityGuard> {
        let logs = zksync_vlog::Logs::try_from(self.clone())?;
        let sentry = Option::<zksync_vlog::Sentry>::try_from(self.clone())?;
        let opentelemetry = Option::<zksync_vlog::OpenTelemetry>::try_from(self.clone())?;

        let guard = zksync_vlog::ObservabilityBuilder::new()
            .with_logs(Some(logs))
            .with_sentry(sentry)
            .with_opentelemetry(opentelemetry)
            .build();
        Ok(guard)
    }
}

impl TryFrom<ObservabilityConfig> for zksync_vlog::Logs {
    type Error = anyhow::Error;

    fn try_from(config: ObservabilityConfig) -> Result<Self, Self::Error> {
        Ok(zksync_vlog::Logs::new(&config.log_format)?.with_log_directives(config.log_directives))
    }
}

impl TryFrom<ObservabilityConfig> for Option<zksync_vlog::Sentry> {
    type Error = anyhow::Error;

    fn try_from(config: ObservabilityConfig) -> Result<Self, Self::Error> {
        Ok(config
            .sentry_url
            .map(|url| zksync_vlog::Sentry::new(&url))
            .transpose()?
            .map(|sentry| sentry.with_environment(config.sentry_environment)))
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
