//! Extensions for the `ObservabilityConfig` to install the observability stack.

use smart_config::{ConfigRepository, ConfigSchema, ConfigSources, DescribeConfig, ParseErrors};

use crate::configs::ObservabilityConfig;

impl ObservabilityConfig {
    pub fn from_sources(sources: ConfigSources) -> Result<Self, ParseErrors> {
        let schema = ConfigSchema::new(&Self::DESCRIPTION, "observability");
        let repo = ConfigRepository::new(&schema).with_all(sources);
        // `unwrap()` is safe: `Self` is the only top-level config, so an error would require for it to have a recursive definition.
        repo.single::<Self>().unwrap().parse()
    }

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

pub trait ParseResultExt<T> {
    fn log_all_errors(self) -> anyhow::Result<T>;
}

impl<T> ParseResultExt<T> for Result<T, ParseErrors> {
    fn log_all_errors(self) -> anyhow::Result<T> {
        match self {
            Ok(val) => Ok(val),
            Err(errors) => {
                for err in errors.iter() {
                    tracing::error!(
                        path = err.path(),
                        origin = %err.origin(),
                        config = err.config().ty.name_in_code(),
                        param = err.param().map(|param| param.rust_field_name),
                        "{}",
                        err.inner()
                    );
                }
                Err(anyhow::anyhow!(
                    "failed parsing config param(s); errors are logged above"
                ))
            }
        }
    }
}
