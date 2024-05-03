//! Schema-guided parsing configs from environment vars.

use std::{any, collections::HashMap};

use anyhow::Context as _;
use serde::de::DeserializeOwned;

use super::{Alias, ConfigSchema};
use crate::metadata::DescribeConfig;

/// Engine powering [`EnvironmentParser`]. Responsible for converting environment `vars` into
/// a deserializable configuration.
pub trait EnvParserEngine {
    fn parse_from_env<C: DescribeConfig>(
        &self,
        prefix: &str,
        vars: &HashMap<String, String>,
    ) -> anyhow::Result<C>;
}

/// Processed environment variables.
#[derive(Debug, Default)]
pub struct Environment {
    vars: HashMap<String, String>,
}

impl Environment {
    /// Captures all environment vars with the specified prefix.
    pub fn prefixed(prefix: &str) -> Self {
        let env = std::env::vars_os().filter_map(|(name, value)| {
            let name = name.into_string().ok()?;
            let value = value.into_string().ok()?;
            Some((name, value))
        });
        Self::from_iter(prefix, env)
    }

    /// Creates an environment from the provided iterator. Mostly useful for testing.
    pub fn from_iter<S>(prefix: &str, env: impl IntoIterator<Item = (S, S)>) -> Self
    where
        S: AsRef<str> + Into<String>,
    {
        let iter = env.into_iter().filter_map(|(name, value)| {
            Some((name.as_ref().strip_prefix(prefix)?.to_owned(), value.into()))
        });
        Self {
            vars: iter.collect(),
        }
    }

    /// Adds additional variables to this environment. This is useful if the added vars don't have the necessary prefix.
    pub fn with_vars(mut self, var_names: &[&str]) -> Self {
        let defined_vars = var_names.iter().filter_map(|&name| {
            let value = std::env::var_os(name)?.into_string().ok()?;
            Some((name.to_owned(), value))
        });
        self.vars.extend(defined_vars);
        self
    }

    fn merge_alias(&mut self, target_prefix: &str, alias: &Alias<()>) {
        for &param_name in &alias.param_names {
            let target_full_name = Self::full_name(target_prefix, param_name);
            if self.vars.contains_key(&target_full_name) {
                continue; // Variable is already set
            }
            let alias_full_name = Self::full_name(alias.prefix, param_name);
            if let Some(value) = self.vars.get(&alias_full_name) {
                self.vars.insert(target_full_name, value.clone());
            }
        }
    }

    /// Converts a logical prefix like 'api.limits' to 'API_LIMITS_'.
    fn env_prefix(prefix: &str) -> String {
        let mut prefix = prefix.to_uppercase().replace('.', "_");
        if !prefix.is_empty() && !prefix.ends_with('_') {
            prefix.push('_');
        }
        prefix
    }

    fn full_name(prefix: &str, name: &str) -> String {
        let name = name.to_uppercase();
        if prefix.is_empty() {
            name
        } else {
            let mut full_name = Self::env_prefix(prefix);
            full_name.push_str(&name);
            full_name
        }
    }

    pub fn parser<E: EnvParserEngine>(
        mut self,
        engine: E,
        schema: &ConfigSchema,
    ) -> EnvironmentParser<'_, E> {
        for config_data in schema.configs.values() {
            for alias in &config_data.aliases {
                self.merge_alias(config_data.prefix, alias);
            }
        }
        EnvironmentParser {
            vars: self.vars,
            schema,
            engine,
        }
    }
}

/// Output of parsing configurations using [`ConfigSchema::parser()`].
#[derive(Debug)]
pub struct EnvironmentParser<'a, E> {
    schema: &'a ConfigSchema,
    vars: HashMap<String, String>,
    engine: E,
}

impl<E: EnvParserEngine> EnvironmentParser<'_, E> {
    /// Parses a configuration.
    pub fn parse<C>(&self) -> anyhow::Result<C>
    where
        C: DescribeConfig + DeserializeOwned,
    {
        let config_data = self
            .schema
            .configs
            .get(&any::TypeId::of::<C>())
            .with_context(|| {
                format!(
                    "Config `{}` is not a part of the schema",
                    any::type_name::<C>()
                )
            })?;
        let env_prefix = Environment::env_prefix(config_data.prefix);
        self.engine
            .parse_from_env(&env_prefix, &self.vars)
            .with_context(|| {
                // FIXME: capture config type name
                format!(
                    "error parsing configuration `FIXME` at `{}` (aliases: {:?})",
                    config_data.prefix, config_data.aliases
                )
            })
    }
}
