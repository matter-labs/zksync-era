use std::{
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::{de::DeserializeOwned, Serialize};
use tokio::fs;
use xshell::Shell;

#[derive(Debug)]
pub struct RawConfig {
    path: PathBuf,
    inner: serde_yaml::Value,
}

impl RawConfig {
    pub async fn read(shell: &Shell, path: &Path) -> anyhow::Result<Self> {
        let path = shell.current_dir().join(path);
        let raw = fs::read_to_string(&path)
            .await
            .with_context(|| format!("failed reading config at `{path:?}`"))?;
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default();
        let inner = match extension {
            "yaml" | "yml" => serde_yaml::from_str(&raw)
                .with_context(|| format!("failed deserializing config at `{path:?}` as YAML"))?,
            "toml" => toml::from_str(&raw)
                .with_context(|| format!("failed deserializing config at `{path:?}` as YAML"))?,
            "json" => serde_json::from_str(&raw)
                .with_context(|| format!("failed deserializing config at `{path:?}` as YAML"))?,
            _ => anyhow::bail!("unsupported config file extension `{extension}`"),
        };
        anyhow::ensure!(
            matches!(&inner, serde_yaml::Value::Mapping(_)),
            "configuration is not a map"
        );
        Ok(Self { inner, path })
    }

    pub fn get_raw(&self, path: &str) -> Option<&serde_yaml::Value> {
        path.split('.')
            .try_fold(&self.inner, |ptr, segment| match ptr {
                serde_yaml::Value::Mapping(map) => map.get(segment),
                _ => None,
            })
    }

    pub fn get_opt<T: DeserializeOwned>(&self, path: &str) -> anyhow::Result<Option<T>> {
        let Some(raw) = self.get_raw(path) else {
            return Ok(None);
        };
        serde_yaml::from_value(raw.clone()).with_context(|| {
            format!(
                "failed deserializing config param `{path}` in `{:?}`",
                self.path
            )
        })
    }

    pub fn get<T: DeserializeOwned>(&self, path: &str) -> anyhow::Result<T> {
        self.get_opt(path)?
            .with_context(|| format!("config param `{path}` is missing in {:?}", self.path))
    }

    pub fn patched(self) -> PatchedConfig {
        PatchedConfig { base: self }
    }
}

/// Mutable YAML configuration file.
#[derive(Debug)]
#[must_use = "Must be persisted"]
pub struct PatchedConfig {
    base: RawConfig,
}

impl PatchedConfig {
    pub fn empty(shell: &Shell, path: &Path) -> Self {
        let path = shell.current_dir().join(path);
        Self {
            base: RawConfig {
                path,
                inner: serde_yaml::Value::Mapping(serde_yaml::Mapping::default()),
            },
        }
    }

    pub fn base(&self) -> &RawConfig {
        &self.base
    }

    pub fn insert(&mut self, key: &str, value: impl Into<serde_yaml::Value>) -> anyhow::Result<()> {
        assert!(!key.is_empty(), "key cannot be empty");
        let value = value.into();

        let serde_yaml::Value::Mapping(map) = &mut self.base.inner else {
            unreachable!(); // checked during initialization
        };
        let mut map = map;
        if let Some((parent_path, last_segment)) = key.rsplit_once('.') {
            for segment in parent_path.split('.') {
                if !map.contains_key(segment) {
                    let new_map = serde_yaml::Mapping::new();
                    map.insert(segment.into(), new_map.into());
                }

                map = match map.get_mut(segment) {
                    Some(serde_yaml::Value::Mapping(child)) => child,
                    Some(_) => anyhow::bail!("Encountered non-map parent when inserting `{key}`"),
                    None => unreachable!(),
                };
            }
            map.insert(last_segment.into(), value);
        } else {
            map.insert(key.into(), value);
        }
        Ok(())
    }

    pub fn insert_yaml(&mut self, key: &str, value: impl Serialize) -> anyhow::Result<()> {
        let value = serde_yaml::to_value(value)
            .unwrap_or_else(|err| panic!("failed serializing config value at `{key}`: {err}"));
        self.insert(key, value)
    }

    pub fn insert_path(&mut self, key: &str, value: &Path) -> anyhow::Result<()> {
        let value = value
            .to_str()
            .with_context(|| format!("path at `{key}` is not UTF-8"))?;
        self.insert(key, value)?;
        Ok(())
    }

    pub fn extend(&mut self, source: serde_yaml::Mapping) {
        let serde_yaml::Value::Mapping(map) = &mut self.base.inner else {
            unreachable!(); // checked during initialization
        };
        map.extend(source);
    }

    pub fn remove(&mut self, key: &str) {
        let serde_yaml::Value::Mapping(map) = &mut self.base.inner else {
            unreachable!(); // checked during initialization
        };
        let mut map = map;

        if let Some((parent_path, last_segment)) = key.rsplit_once('.') {
            for segment in parent_path.split('.') {
                map = match map.get_mut(segment) {
                    Some(serde_yaml::Value::Mapping(child)) => child,
                    _ => return,
                };
            }
            map.remove(last_segment);
        } else {
            map.remove(key);
        }
    }

    pub async fn save(self) -> anyhow::Result<()> {
        let path = self.base.path;
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default();
        let contents = match extension {
            "yaml" | "yml" => serde_yaml::to_string(&self.base.inner)
                .with_context(|| format!("failed serializing config at `{path:?}` as YAML"))?,
            "json" => serde_json::to_string_pretty(&self.base.inner)
                .with_context(|| format!("failed serializing config at `{path:?}` as YAML"))?,
            "toml" => toml::to_string_pretty(&self.base.inner)
                .with_context(|| format!("failed serializing config at `{path:?}` as YAML"))?,
            _ => {
                anyhow::bail!("unsupported config file extension `{extension}`");
            }
        };
        tokio::task::spawn_blocking(move || {
            let (target, permissions) = match std::fs::canonicalize(&path) {
                Ok(target) => {
                    let permissions = std::fs::metadata(&target)?.permissions();
                    (target, Some(permissions))
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => (path.clone(), None),
                Err(err) => return Err(err.into()),
            };
            let mut file = tempfile::Builder::new().make_in(
                target.parent().context("config path has no parent")?,
                |path| {
                    std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(path)
                },
            )?;
            if let Some(permissions) = permissions {
                file.as_file().set_permissions(permissions)?;
            }
            file.write_all(contents.as_bytes())?;
            file.persist(&target)
                .with_context(|| format!("failed writing config to `{path:?}`"))?;
            anyhow::Ok(())
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    #[tokio::test]
    async fn creates_config_with_default_permissions() {
        let shell = Shell::new().unwrap();
        let dir = shell.create_temp_dir().unwrap();
        let reference = dir.path().join("reference.yaml");
        shell.write_file(&reference, "prover: {}\n").unwrap();
        let path = dir.path().join("general.yaml");
        let mut config = PatchedConfig::empty(&shell, &path);
        config
            .insert("prover.setup_data_path", "data/keys")
            .unwrap();
        config.save().await.unwrap();

        let config = RawConfig::read(&shell, &path).await.unwrap();
        assert_eq!(
            config.get::<String>("prover.setup_data_path").unwrap(),
            "data/keys"
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions(),
            std::fs::metadata(&reference).unwrap().permissions()
        );
    }

    #[tokio::test]
    async fn in_flight_read_sees_complete_previous_config() {
        let shell = Shell::new().unwrap();
        let dir = shell.create_temp_dir().unwrap();
        let path = dir.path().join("general.yaml");
        let original = "prover:\n  setup_data_path: data/keys\n";
        shell.write_file(&path, original).unwrap();

        let mut reader = std::fs::File::open(&path).unwrap();
        let mut prefix = [0; 8];
        reader.read_exact(&mut prefix).unwrap();
        let mut config = RawConfig::read(&shell, &path).await.unwrap().patched();
        config
            .insert("prover.setup_data_path", "prover/data/keys")
            .unwrap();
        config.save().await.unwrap();

        let mut snapshot = String::from_utf8(prefix.to_vec()).unwrap();
        reader.read_to_string(&mut snapshot).unwrap();
        assert_eq!(snapshot, original);
        let config = RawConfig::read(&shell, &path).await.unwrap();
        assert_eq!(
            config.get::<String>("prover.setup_data_path").unwrap(),
            "prover/data/keys"
        );
    }
}
