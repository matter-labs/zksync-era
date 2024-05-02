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

type ParseFn = fn(&str, &HashMap<String, String>) -> envy::Result<Box<dyn any::Any>>;

#[derive(Debug)]
struct ConfigData {
    prefix: &'static str,
    aliases: Vec<Alias<()>>,
    metadata: &'static ConfigMetadata,
    parser: ParseFn,
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
                parser: |prefix, env| {
                    envy::prefixed(prefix)
                        .from_iter::<_, C>(env.clone())
                        .map(|config| Box::new(config) as Box<dyn any::Any>)
                },
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
        println!("{prefix}{}", param.name);

        let aliases = param.aliases.iter().copied();
        let global_aliases = config_data.aliases.iter().flat_map(|alias| {
            aliases
                .clone()
                .chain([param.name])
                .filter(|name| alias.param_names.contains(name))
        });
        for alias in aliases.clone().chain(global_aliases) {
            println!("{alias} [deprecated]");
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

    /// Parses the provided environment according to this schema.
    pub fn parse_env(&self, mut env: Environment) -> anyhow::Result<ParsedConfigs> {
        for config_data in self.configs.values() {
            for alias in &config_data.aliases {
                env.merge_alias(config_data.prefix, alias);
            }
        }

        let configs = self.configs.iter().map(|(type_id, config_data)| {
            let env_prefix = Environment::env_prefix(config_data.prefix);
            let parsed = (config_data.parser)(&env_prefix, &env.vars).with_context(|| {
                // FIXME: capture config type name
                format!(
                    "error parsing configuration `FIXME` at `{}` (aliases: {:?})",
                    config_data.prefix, config_data.aliases
                )
            })?;
            Ok((*type_id, parsed))
        });
        let configs = configs.collect::<anyhow::Result<_>>()?;
        Ok(ParsedConfigs { configs })
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
}

/// Output of parsing configurations using [`ConfigSchema::parse_env()`].
#[derive(Debug)]
pub(super) struct ParsedConfigs {
    configs: HashMap<any::TypeId, Box<dyn any::Any>>,
}

impl ParsedConfigs {
    /// Takes a configuration. It should be a part of the [`ConfigSchema`] that produced this object;
    /// otherwise, this method will panic.
    pub fn take<C: DescribeConfig>(&mut self) -> C {
        *self
            .configs
            .remove(&any::TypeId::of::<C>())
            .unwrap_or_else(|| {
                panic!(
                    "Config `{}` is not a part of the schema",
                    any::type_name::<C>()
                )
            })
            .downcast::<C>()
            .unwrap() // Safe by design
    }
}
