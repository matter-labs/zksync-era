use std::{
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::Context;
use smart_config::{ConfigRepository, ConfigSchema, ConfigSource, Environment, Prefixed, Yaml};

/// Wrapper around configuration sources.
#[derive(Debug, Default)]
pub struct ConfigSources(pub(crate) smart_config::ConfigSources);

impl ConfigSources {
    /// Adds a new YAML configuration source.
    pub fn with_yaml(mut self, path: &Path) -> anyhow::Result<Self> {
        self.0.push(ConfigFilePaths::read_yaml(path)?);
        Ok(self)
    }

    /// Pushes a config source.
    pub fn push(&mut self, source: impl ConfigSource) {
        self.0.push(source);
    }

    /// Builds the repository with the specified config schema. Deserialization options are tuned to be backward-compatible
    /// with the existing file-based configs (e.g., coerce enum variant names).
    pub fn build_repository(self, schema: &ConfigSchema) -> ConfigRepository<'_> {
        let mut repo = ConfigRepository::new(schema);
        repo.deserializer_options().coerce_variant_names = true;
        repo.deserializer_options().coerce_serde_enums = true;
        repo.with_all(self.0)
    }
}

#[derive(Debug, Default)]
pub struct ConfigFilePaths {
    pub general: Option<PathBuf>,
    pub secrets: Option<PathBuf>,
    pub contracts: Option<PathBuf>,
    pub genesis: Option<PathBuf>,
    pub wallets: Option<PathBuf>,
    pub consensus: Option<PathBuf>,
    pub external_node: Option<PathBuf>,
}

impl ConfigFilePaths {
    /// This method is blocking.
    pub fn read_yaml(path: &Path) -> anyhow::Result<Yaml> {
        let file =
            fs::File::open(path).with_context(|| format!("failed opening config file {path:?}"))?;
        let raw: serde_yaml::Mapping = serde_yaml::from_reader(io::BufReader::new(file))
            .with_context(|| format!("failed reading YAML map from {path:?}"))?;
        let filename = path.as_os_str().to_string_lossy();
        Yaml::new(&filename, raw)
            .with_context(|| format!("failed digesting YAML map from {path:?}"))
    }

    /// **Important.** This method is blocking.
    pub fn into_config_sources(self, env_prefix: &str) -> anyhow::Result<ConfigSources> {
        let mut sources = smart_config::ConfigSources::default();

        if let Some(path) = &self.general {
            sources.push(Self::read_yaml(path)?);
        }
        if let Some(path) = &self.secrets {
            sources.push(Self::read_yaml(path)?);
        }

        // Prefixed sources
        if let Some(path) = &self.contracts {
            sources.push(Prefixed::new(Self::read_yaml(path)?, "contracts"));
        }
        if let Some(path) = &self.genesis {
            sources.push(Prefixed::new(Self::read_yaml(path)?, "genesis"));
        }
        if let Some(path) = &self.wallets {
            sources.push(Prefixed::new(Self::read_yaml(path)?, "wallets"));
        }
        if let Some(path) = &self.external_node {
            sources.push(Prefixed::new(Self::read_yaml(path)?, "networks"));
        }
        if let Some(path) = &self.consensus {
            sources.push(Prefixed::new(Self::read_yaml(path)?, "consensus"));
        }

        let mut environment = Environment::prefixed(env_prefix);
        if let Err(err) = environment.coerce_json() {
            // We don't consider coercion errors fatal, but they obviously signify something wrong with the setup.
            tracing::error!("{err}");
        }
        sources.push(environment);
        Ok(ConfigSources(sources))
    }
}
