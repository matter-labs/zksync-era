//! CLI Command for printing configs.

use std::{
    any,
    collections::{HashMap, HashSet},
    marker::PhantomData,
};

use anyhow::Context;
use serde::de::DeserializeOwned;
use zksync_config::metadata::{ConfigMetadata, DescribeConfig, ParamMetadata};

const INDENT: &str = "    ";

/// Alias specification for a config.
#[derive(Debug)]
pub(super) struct Alias<C> {
    prefix: &'static str,
    param_names: HashSet<&'static str>,
    _config: PhantomData<C>,
}

impl<C: DescribeConfig> Alias<C> {
    /// Specifies that aliased parameters (which is all config params) start at the specified `prefix`.
    pub fn prefix(prefix: &'static str) -> Self {
        Self {
            prefix,
            param_names: C::describe_config()
                .params
                .iter()
                .flat_map(|param| param.aliases.iter().copied().chain([param.name]))
                .collect(),
            _config: PhantomData,
        }
    }

    /// Excludes parameters from this alias rule according to the provided predicate.
    #[must_use]
    pub fn exclude(mut self, mut predicate: impl FnMut(&str) -> bool) -> Self {
        self.param_names.retain(|name| !predicate(name));
        self
    }

    fn drop_type_param(self) -> Alias<()> {
        Alias {
            prefix: self.prefix,
            param_names: self.param_names,
            _config: PhantomData,
        }
    }
}

#[derive(Debug)]
struct ConfigData {
    prefix: &'static str,
    aliases: Vec<Alias<()>>,
    metadata: &'static ConfigMetadata,
}

/// Schema for configuration. Can contain multiple configs bound to different "locations".
#[derive(Default, Debug)]
pub(super) struct ConfigSchema {
    configs: HashMap<any::TypeId, ConfigData>,
}

impl ConfigSchema {
    /// Inserts a new configuration type at the specified place.
    #[must_use]
    pub fn insert<C>(self, prefix: &'static str) -> Self
    where
        C: DescribeConfig + DeserializeOwned,
    {
        self.insert_aliased::<C>(prefix, [])
    }

    /// Inserts a new configuration type at the specified place with potential aliases.
    #[must_use]
    pub fn insert_aliased<C>(
        mut self,
        prefix: &'static str,
        aliases: impl IntoIterator<Item = Alias<C>>,
    ) -> Self
    where
        C: DescribeConfig + DeserializeOwned,
    {
        self.configs.insert(
            any::TypeId::of::<C>(),
            ConfigData {
                prefix,
                aliases: aliases.into_iter().map(Alias::drop_type_param).collect(),
                metadata: C::describe_config(),
            },
        );
        self
    }

    /// Prints help about this schema.
    pub fn print_help(&self, mut param_filter: impl FnMut(&ParamMetadata) -> bool) {
        for config_data in self.configs.values() {
            let filtered_params: Vec<_> = config_data
                .metadata
                .params
                .iter()
                .filter(|&param| param_filter(param))
                .collect();
            if filtered_params.is_empty() {
                return;
            }

            println!("{}\n", config_data.metadata.help);
            for param in filtered_params {
                Self::print_parameter(param, config_data);
                println!();
            }
        }
    }

    fn print_parameter(param: &ParamMetadata, config_data: &ConfigData) {
        let prefix = config_data.prefix;
        let prefix_sep = if prefix.is_empty() || prefix.ends_with('.') {
            ""
        } else {
            "."
        };
        println!("{prefix}{prefix_sep}{}", param.name);

        let local_aliases = param.aliases.iter().copied();
        let global_aliases = config_data.aliases.iter().flat_map(|alias| {
            local_aliases
                .clone()
                .chain([param.name])
                .filter_map(|name| {
                    alias
                        .param_names
                        .contains(name)
                        .then_some((alias.prefix, name))
                })
        });
        let local_aliases = local_aliases.clone().map(|name| (prefix, name));
        for (prefix, alias) in local_aliases.chain(global_aliases) {
            let prefix_sep = if prefix.is_empty() || prefix.ends_with('.') {
                ""
            } else {
                "."
            };
            println!("{prefix}{prefix_sep}{alias}");
        }

        let ty = if let Some(kind) = param.base_type.kind() {
            format!("{kind} [Rust: {}]", param.ty.name_in_code())
        } else {
            param.ty.name_in_code().to_owned()
        };
        let default = if let Some(default) = param.default_value() {
            format!(", default: {default:?}")
        } else {
            String::new()
        };
        let unit = if let Some(unit) = &param.unit {
            format!(" [unit: {unit}]")
        } else {
            String::new()
        };
        println!("{INDENT}Type: {ty}{default}{unit}");

        if !param.help.is_empty() {
            for line in param.help.lines() {
                println!("{INDENT}{line}");
            }
        }
    }
}

/// Processed environment variables.
#[derive(Debug, Default)]
pub(super) struct Environment {
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

    pub fn with_schema(mut self, schema: &ConfigSchema) -> PreparedEnvironment<'_> {
        for config_data in schema.configs.values() {
            for alias in &config_data.aliases {
                self.merge_alias(config_data.prefix, alias);
            }
        }
        PreparedEnvironment {
            vars: self.vars,
            schema,
        }
    }
}

/// Output of parsing configurations using [`ConfigSchema::parse_env()`].
#[derive(Debug)]
pub(super) struct PreparedEnvironment<'a> {
    schema: &'a ConfigSchema,
    vars: HashMap<String, String>,
}

impl PreparedEnvironment<'_> {
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
        envy::prefixed(env_prefix)
            .from_iter(self.vars.clone())
            .with_context(|| {
                // FIXME: capture config type name
                format!(
                    "error parsing configuration `FIXME` at `{}` (aliases: {:?})",
                    config_data.prefix, config_data.aliases
                )
            })
    }
}
