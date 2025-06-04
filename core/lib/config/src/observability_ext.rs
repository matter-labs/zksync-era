//! Extensions for the `ObservabilityConfig` to install the observability stack.

use std::{any, any::Any, mem};

use anyhow::Context;
use smart_config::{
    metadata::ConfigMetadata,
    value,
    visit::{ConfigVisitor, VisitConfig},
    ConfigRepository, ConfigSchema, DescribeConfig, DeserializeConfig, ParseErrors,
};
use zksync_vlog::prometheus::PrometheusExporterConfig;

use crate::{
    configs::{observability::LogFormat, ObservabilityConfig, PrometheusConfig},
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
        repo.single().unwrap().parse().map_err(log_all_errors)
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
        let logs = logs_transform(zksync_vlog::Logs::from(self.clone()));
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

fn log_all_errors(errors: ParseErrors) -> anyhow::Error {
    const MAX_DISPLAYED_ERRORS: usize = 5;

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

    anyhow::anyhow!(
        "failed parsing config param(s): {error_count} error(s) in total{maybe_truncation_message}\n{displayed_errors}"
    )
}

pub trait ConfigRepositoryExt {
    /// Parses a configuration from this repo. The configuration must have a unique mounting point.
    fn parse<C: DeserializeConfig>(&self) -> anyhow::Result<C>;

    /// Parses an optional configuration from this repo. The configuration must have a unique mounting point.
    fn parse_opt<C: DeserializeConfig>(&self) -> anyhow::Result<Option<C>>;

    fn parse_at<C: DeserializeConfig>(&self, prefix: &str) -> anyhow::Result<C>;
}

impl ConfigRepositoryExt for ConfigRepository<'_> {
    /// Parses a configuration from this repo. The configuration must have a unique mounting point.
    fn parse<C: DeserializeConfig>(&self) -> anyhow::Result<C> {
        let config_parser = self.single::<C>()?;
        let prefix = config_parser.config().prefix();
        let config = config_parser.parse().map_err(log_all_errors)?;
        ObservabilityVisitor::visit(self, prefix, &config);
        Ok(config)
    }

    /// Parses an optional configuration from this repo. The configuration must have a unique mounting point.
    fn parse_opt<C: DeserializeConfig>(&self) -> anyhow::Result<Option<C>> {
        let config_parser = self.single::<C>()?;
        let prefix = config_parser.config().prefix();
        let maybe_config = config_parser.parse_opt().map_err(log_all_errors)?;
        if let Some(config) = &maybe_config {
            ObservabilityVisitor::visit(self, prefix, config);
        }
        Ok(maybe_config)
    }

    fn parse_at<C: DeserializeConfig>(&self, prefix: &str) -> anyhow::Result<C> {
        let config_parser = self.get(prefix).with_context(|| {
            format!(
                "config `{}` is missing at `{prefix}`",
                any::type_name::<C>()
            )
        })?;
        let prefix = config_parser.config().prefix();
        let config = config_parser.parse().map_err(log_all_errors)?;
        ObservabilityVisitor::visit(self, prefix, &config);
        Ok(config)
    }
}

#[derive(Debug)]
struct ObservabilityVisitor<'a> {
    source: Option<&'a value::Map>,
    metadata: &'static ConfigMetadata,
    config_prefix: String,
}

impl<'a> ObservabilityVisitor<'a> {
    fn visit<C: DeserializeConfig>(repo: &'a ConfigRepository<'_>, prefix: &str, config: &C) {
        let source = repo
            .merged()
            .pointer(prefix)
            .and_then(|val| val.inner.as_object());
        let mut this = Self {
            source,
            metadata: &C::DESCRIPTION,
            config_prefix: prefix.to_owned(),
        };
        (C::DESCRIPTION.visitor)(config, &mut this);
    }

    fn join_path(prefix: &str, suffix: &str) -> String {
        if prefix.is_empty() {
            suffix.to_owned()
        } else {
            format!("{prefix}.{suffix}")
        }
    }
}

impl ConfigVisitor for ObservabilityVisitor<'_> {
    fn visit_tag(&mut self, variant_index: usize) {
        let tag = self.metadata.tag.unwrap();
        let param = tag.param;
        let tag_variant = &tag.variants[variant_index];
        let origin = self
            .source
            .and_then(|map| Some(map.get(param.name)?.origin.as_ref()));
        let value = tag_variant.name;
        tracing::debug!(
            param = param.name,
            path = Self::join_path(&self.config_prefix, param.name),
            rust_name = param.rust_field_name,
            config = self.metadata.ty.name_in_code(),
            origin = origin.map(tracing::field::display),
            value,
            "parsed config tag"
        );
    }

    fn visit_param(&mut self, param_index: usize, value: &dyn Any) {
        let param = &self.metadata.params[param_index];
        let rust_name = (param.rust_field_name != param.name).then_some(param.rust_field_name);
        let origin = self
            .source
            .and_then(|map| Some(map.get(param.name)?.origin.as_ref()));
        let (is_secret, is_default, value) = if param.type_description().contains_secrets() {
            // We don't want to serialize secrets to check defaults in the same way as non-secret values,
            // but if there's no `origin`, we can be sure that the param is set to the default value (usually `None`).
            let is_default = origin.is_none().then_some(true);
            (true, is_default, None)
        } else {
            let json = param.deserializer.serialize_param(value);
            let is_default = param
                .default_value_json()
                .map(|default_val| default_val == json);
            (false, is_default, Some(json))
        };

        tracing::debug!(
            param = param.name,
            rust_name,
            path = Self::join_path(&self.config_prefix, param.name),
            config = self.metadata.ty.name_in_code(),
            origin = origin.map(tracing::field::display),
            value = value.map(tracing::field::display),
            is_secret,
            is_default,
            "parsed config param"
        );
    }

    fn visit_nested_config(&mut self, config_index: usize, config: &dyn VisitConfig) {
        let nested_metadata = &self.metadata.nested_configs[config_index];
        let prev_metadata = mem::replace(&mut self.metadata, nested_metadata.meta);

        if nested_metadata.name.is_empty() {
            config.visit_config(self);
        } else {
            let nested_prefix = Self::join_path(&self.config_prefix, nested_metadata.name);
            let prev_prefix = mem::replace(&mut self.config_prefix, nested_prefix);
            let new_source = self
                .source
                .and_then(|map| map.get(nested_metadata.name)?.inner.as_object());
            let prev_source = mem::replace(&mut self.source, new_source);
            config.visit_config(self);
            self.source = prev_source;
            self.config_prefix = prev_prefix;
        }

        self.metadata = prev_metadata;
    }
}
