use std::{any, any::Any, collections::HashMap, mem};

use anyhow::Context;
use serde::Serialize;
use smart_config::{
    metadata::ConfigMetadata,
    value,
    visit::{ConfigVisitor, VisitConfig},
    DeserializeConfig, ParseErrors,
};

pub(crate) fn log_all_errors(errors: ParseErrors) -> anyhow::Error {
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

#[derive(Debug, Serialize)]
pub(crate) struct ParsedParam {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) origin: Option<String>,
    #[serde(skip_serializing_if = "ParsedParam::is_false")]
    pub(crate) is_secret: bool,
    #[serde(skip_serializing_if = "ParsedParam::is_true")]
    pub(crate) is_default: bool,
}

impl ParsedParam {
    fn is_false(&flag: &bool) -> bool {
        !flag
    }

    fn is_true(&flag: &bool) -> bool {
        flag
    }
}

/// Opaque representation of captured config parameters.
#[derive(Debug, Default, Serialize)]
#[serde(transparent)]
pub struct CapturedParams(pub(crate) HashMap<String, ParsedParam>);

impl CapturedParams {
    /// Returns the number of the contained params.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Checks whether this contains any params.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Configuration repository wrapper providing convenient methods.
#[derive(Debug)]
pub struct ConfigRepository<'a> {
    inner: smart_config::ConfigRepository<'a>,
    parsed_params: Option<CapturedParams>,
}

impl ConfigRepository<'_> {
    pub fn capture_parsed_params(&mut self) {
        if self.parsed_params.is_none() {
            self.parsed_params = Some(CapturedParams::default());
        }
    }

    /// Parses a configuration from this repo. The configuration must have a unique mounting point.
    pub fn parse<C: DeserializeConfig>(&mut self) -> anyhow::Result<C> {
        let config_parser = self.inner.single::<C>()?;
        let prefix = config_parser.config().prefix();
        let config = config_parser.parse().map_err(log_all_errors)?;
        ObservabilityVisitor::visit(&self.inner, self.parsed_params.as_mut(), prefix, &config);
        Ok(config)
    }

    /// Parses an optional configuration from this repo. The configuration must have a unique mounting point.
    pub fn parse_opt<C: DeserializeConfig>(&mut self) -> anyhow::Result<Option<C>> {
        let config_parser = self.inner.single::<C>()?;
        let prefix = config_parser.config().prefix();
        let maybe_config = config_parser.parse_opt().map_err(log_all_errors)?;
        if let Some(config) = &maybe_config {
            ObservabilityVisitor::visit(&self.inner, self.parsed_params.as_mut(), prefix, config);
        }
        Ok(maybe_config)
    }

    pub fn parse_at<C: DeserializeConfig>(&mut self, prefix: &str) -> anyhow::Result<C> {
        let config_parser = self.inner.get(prefix).with_context(|| {
            format!(
                "config `{}` is missing at `{prefix}`",
                any::type_name::<C>()
            )
        })?;
        let prefix = config_parser.config().prefix();
        let config = config_parser.parse().map_err(log_all_errors)?;
        ObservabilityVisitor::visit(&self.inner, self.parsed_params.as_mut(), prefix, &config);
        Ok(config)
    }

    /// Returns all captured parsed params, or an empty container if none were captured. Also, observes
    /// the returned params as `INFO` metrics.
    pub fn into_captured_params(self) -> CapturedParams {
        let params = self.parsed_params.unwrap_or_default();
        #[cfg(feature = "observability_ext")]
        crate::observability_ext::METRICS.observe(&params);
        params
    }
}

impl<'a> From<smart_config::ConfigRepository<'a>> for ConfigRepository<'a> {
    fn from(inner: smart_config::ConfigRepository<'a>) -> Self {
        Self {
            inner,
            parsed_params: None,
        }
    }
}

impl<'a> From<ConfigRepository<'a>> for smart_config::ConfigRepository<'a> {
    fn from(repo: ConfigRepository<'a>) -> Self {
        repo.inner
    }
}

#[derive(Debug)]
struct ObservabilityVisitor<'a> {
    source: Option<&'a value::Map>,
    parsed_params: Option<&'a mut CapturedParams>,
    metadata: &'static ConfigMetadata,
    config_prefix: String,
}

impl<'a> ObservabilityVisitor<'a> {
    fn visit<C: DeserializeConfig>(
        repo: &'a smart_config::ConfigRepository<'_>,
        parsed_params: Option<&'a mut CapturedParams>,
        prefix: &str,
        config: &C,
    ) {
        let source = repo
            .merged()
            .pointer(prefix)
            .and_then(|val| val.inner.as_object());
        let mut this = Self {
            source,
            parsed_params,
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

        let path = Self::join_path(&self.config_prefix, param.name);
        tracing::debug!(
            param = param.name,
            rust_name,
            path,
            config = self.metadata.ty.name_in_code(),
            origin = origin.map(tracing::field::display),
            value = value.as_ref().map(tracing::field::display),
            is_secret,
            is_default,
            "parsed config param"
        );

        if let Some(params) = self.parsed_params.as_deref_mut() {
            params.0.insert(
                path,
                ParsedParam {
                    value,
                    origin: origin.map(ToString::to_string),
                    is_secret,
                    is_default: is_default.unwrap_or(false),
                },
            );
        }
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
