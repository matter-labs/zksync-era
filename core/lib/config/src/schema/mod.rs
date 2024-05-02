//! Configuration schema.

use std::{
    any,
    collections::{HashMap, HashSet},
    marker::PhantomData,
};

use serde::de::DeserializeOwned;

use crate::metadata::{ConfigMetadata, DescribeConfig, ParamMetadata};

pub mod env;

const INDENT: &str = "    ";

/// Alias specification for a config.
#[derive(Debug)]
pub struct Alias<C> {
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
pub struct ConfigSchema {
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
