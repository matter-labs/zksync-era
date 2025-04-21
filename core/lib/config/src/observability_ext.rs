//! Extensions for the `ObservabilityConfig` to install the observability stack.

use smart_config::{ConfigRepository, ConfigSchema, DescribeConfig, ParseErrors};
use zksync_vlog::prometheus::PrometheusExporterConfig;

use crate::{
    configs::{ObservabilityConfig, PrometheusConfig},
    sources::ConfigSources,
};

impl ConfigSources {
    /// Returns the observability config. It should be used to install observability early in the executable lifecycle.
    pub fn observability(&self) -> anyhow::Result<ObservabilityConfig> {
        let schema = ConfigSchema::new(&ObservabilityConfig::DESCRIPTION, "observability");
        let mut repo = ConfigRepository::new(&schema).with_all(self.0.clone());
        repo.deserializer_options().coerce_variant_names = true;
        // - `unwrap()` is safe: `Self` is the only top-level config, so an error would require for it to have a recursive definition.
        // - While logging is not enabled at this point, we use `log_all_errors()` for more intelligent error summarization.
        repo.single().unwrap().parse().log_all_errors()
    }

    /// Builds the repository with the specified config schema. Deserialization options are tuned to be backward-compatible
    /// with the existing file-based configs (e.g., coerce enum variant names).
    pub fn build_repository(self, schema: &ConfigSchema) -> ConfigRepository<'_> {
        let mut repo = ConfigRepository::new(schema).with_all(self.0);
        repo.deserializer_options().coerce_variant_names = true;
        repo
    }
}

impl ObservabilityConfig {
    /// Installs the observability stack based on the configuration.
    pub fn install(self) -> anyhow::Result<zksync_vlog::ObservabilityGuard> {
        self.install_with_logs(std::convert::identity)
    }

    pub fn install_with_logs(
        self,
        logs_transform: impl FnOnce(zksync_vlog::Logs) -> zksync_vlog::Logs,
    ) -> anyhow::Result<zksync_vlog::ObservabilityGuard> {
        let logs = logs_transform(zksync_vlog::Logs::try_from(self.clone())?);
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

impl TryFrom<ObservabilityConfig> for zksync_vlog::Logs {
    type Error = anyhow::Error;

    fn try_from(config: ObservabilityConfig) -> Result<Self, Self::Error> {
        Ok(zksync_vlog::Logs::new(&config.log_format)?
            .with_log_directives(Some(config.log_directives)))
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

pub trait ParseResultExt<T> {
    fn log_all_errors(self) -> anyhow::Result<T>;
}

impl<T> ParseResultExt<T> for Result<T, ParseErrors> {
    fn log_all_errors(self) -> anyhow::Result<T> {
        const MAX_DISPLAYED_ERRORS: usize = 5;

        match self {
            Ok(val) => Ok(val),
            Err(errors) => {
                let mut displayed_errors = String::new();
                let mut error_count = 0;
                for (i, err) in errors.iter().enumerate() {
                    tracing::error!(
                        path = err.path(),
                        origin = %err.origin(),
                        config = err.config().ty.name_in_code(),
                        param = err.param().map(|param| param.rust_field_name),
                        "{}",
                        err.inner()
                    );

                    if i < MAX_DISPLAYED_ERRORS {
                        displayed_errors += &format!("{}. {err}\n", i + 1);
                    }
                    error_count += 1;
                }

                let maybe_truncation_message = if error_count > MAX_DISPLAYED_ERRORS {
                    format!("; showing first {MAX_DISPLAYED_ERRORS} (all errors are logged at ERROR level)")
                } else {
                    String::new()
                };

                Err(anyhow::anyhow!(
                    "failed parsing config param(s): {error_count} error(s) in total{maybe_truncation_message}\n{displayed_errors}"
                ))
            }
        }
    }
}
