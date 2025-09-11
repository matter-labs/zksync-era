use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{
    Value as JsonValue,
    Value::{Array, Object},
};
use xshell::Shell;
use zkstack_cli_common::files::{
    read_json_file, read_toml_file, read_yaml_file, save_json_file, save_toml_file, save_yaml_file,
};

// Configs that we use only inside ZK Stack CLI, we don't have protobuf implementation for them.
pub trait FileConfigTrait {}

pub trait FileConfigWithDefaultName {
    const FILE_NAME: &'static str;

    fn get_path_with_base_path(base_path: impl AsRef<Path>) -> PathBuf {
        base_path.as_ref().join(Self::FILE_NAME)
    }
}

impl<T: Serialize + FileConfigTrait> SaveConfig for T {
    fn save(&self, shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<()> {
        save_with_comment(shell, path, self, "")
    }
}

impl<T> ReadConfigWithBasePath for T
where
    T: FileConfigWithDefaultName + Clone + ReadConfig,
{
    fn read_with_base_path(shell: &Shell, base_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        <Self as ReadConfig>::read(shell, base_path.as_ref().join(Self::FILE_NAME))
    }
}

impl<T> SaveConfigWithBasePath for T where T: FileConfigWithDefaultName + SaveConfig {}

impl<T> SaveConfigWithCommentAndBasePath for T where
    T: FileConfigWithDefaultName + Serialize + SaveConfig
{
}

/// Reads a config file from a given path, correctly parsing file extension.
/// Supported file extensions are: `yaml`, `yml`, `toml`, `json`.
pub trait ReadConfig: Sized {
    fn read(shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<Self>;
}

impl<T> ReadConfig for T
where
    T: DeserializeOwned + Clone + FileConfigTrait,
{
    fn read(shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let error_context = || format!("Failed to parse config file {:?}.", path.as_ref());

        match path.as_ref().extension().and_then(|ext| ext.to_str()) {
            Some("yaml") | Some("yml") => read_yaml_file(shell, &path).with_context(error_context),
            Some("toml") => read_toml_file(shell, &path).with_context(error_context),
            Some("json") => read_json_file(shell, &path).with_context(error_context),
            _ => bail!(format!(
                "Unsupported file extension for config file {:?}.",
                path.as_ref()
            )),
        }
    }
}

/// Reads a config file from a base path, correctly parsing file extension.
/// Supported file extensions are: `yaml`, `yml`, `toml`, `json`.
pub trait ReadConfigWithBasePath: ReadConfig + FileConfigWithDefaultName + Clone {
    fn read_with_base_path(shell: &Shell, base_path: impl AsRef<Path>) -> anyhow::Result<Self>;
}

/// Saves a config file to a given path, correctly parsing file extension.
/// Supported file extensions are: `yaml`, `yml`, `toml`, `json`.
pub trait SaveConfig {
    fn save(&self, shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<()>;
}

/// Saves a config file from a base path, correctly parsing file extension.
/// Supported file extensions are: `yaml`, `yml`, `toml`, `json`.
pub trait SaveConfigWithBasePath: SaveConfig + FileConfigWithDefaultName {
    fn save_with_base_path(
        &self,
        shell: &Shell,
        base_path: impl AsRef<Path>,
    ) -> anyhow::Result<()> {
        <Self as SaveConfig>::save(self, shell, base_path.as_ref().join(Self::FILE_NAME))
    }
}

/// Saves a config file to a given path, correctly parsing file extension.
/// Supported file extensions are: `yaml`, `yml`, `toml`.
pub trait SaveConfigWithComment: Sized {
    fn save_with_comment(
        &self,
        shell: &Shell,
        path: impl AsRef<Path>,
        comment: &str,
    ) -> anyhow::Result<()>;
}

impl<T: Sized + Serialize> SaveConfigWithComment for T {
    fn save_with_comment(
        &self,
        shell: &Shell,
        path: impl AsRef<Path>,
        comment: &str,
    ) -> anyhow::Result<()> {
        let comment_char = match path.as_ref().extension().and_then(|ext| ext.to_str()) {
            Some("yaml") | Some("yml") | Some("toml") => "#",
            _ => bail!("Unsupported file extension for config file."),
        };
        let comment_lines = comment
            .lines()
            .map(|line| format!("{comment_char} {line}"))
            .chain(std::iter::once("".to_string())) // Add a newline after the comment
            .collect::<Vec<_>>()
            .join("\n");

        save_with_comment(shell, path, self, comment_lines)
    }
}

/// Saves a config file from a base path, correctly parsing file extension.
/// Supported file extensions are: `yaml`, `yml`, `toml`.
pub trait SaveConfigWithCommentAndBasePath:
    SaveConfigWithComment + FileConfigWithDefaultName
{
    fn save_with_comment_and_base_path(
        &self,
        shell: &Shell,
        base_path: impl AsRef<Path>,
        comment: &str,
    ) -> anyhow::Result<()> {
        <Self as SaveConfigWithComment>::save_with_comment(
            self,
            shell,
            base_path.as_ref().join(Self::FILE_NAME),
            comment,
        )
    }
}

fn save_with_comment(
    shell: &Shell,
    path: impl AsRef<Path>,
    data: impl Serialize,
    comment: impl ToString,
) -> anyhow::Result<()> {
    match path.as_ref().extension().and_then(|ext| ext.to_str()) {
        Some("yaml") | Some("yml") => save_yaml_file(shell, path, data, comment)?,
        Some("toml") => save_toml_file(shell, path, data, comment)?,
        Some("json") => save_json_file(shell, path, data)?,
        _ => bail!("Unsupported file extension for config file."),
    }
    Ok(())
}

/// Merge & save: reads existing file (any of yaml/yml/toml/json), deep-merges `self` into it,
/// then writes back in the same format. If the file doesn't exist, behaves like `save`.
pub trait MergeAndSaveWithBasePath:
    Serialize + FileConfigWithDefaultName + ReadConfigWithBasePath + Clone + SaveConfig
{
    fn merge_and_save_with_base_path(
        &self,
        shell: &Shell,
        base_path: impl AsRef<Path>,
    ) -> anyhow::Result<()> {
        let path = base_path.as_ref().join(Self::FILE_NAME);

        if path.exists() {
            let existing = read_value(shell, &path)?;
            let incoming = serde_json::to_value(self)?;
            let merged = deep_merge_json(existing, incoming);
            save_with_comment(shell, &path, merged, "") // still works fine with empty comment
        } else {
            <Self as SaveConfig>::save(self, shell, &path)
        }
    }
}

impl<T> MergeAndSaveWithBasePath for T where
    T: Serialize + FileConfigWithDefaultName + ReadConfigWithBasePath + Clone + SaveConfig
{
}

// ---------- Deep-merge helper (object-wise) ----------
fn deep_merge_json(base: JsonValue, patch: JsonValue) -> JsonValue {
    match (base, patch) {
        (Object(mut a), Object(b)) => {
            for (k, v_new) in b {
                let v_merged = match (a.remove(&k), v_new) {
                    (Some(old @ Object(_)), new @ Object(_)) => deep_merge_json(old, new),
                    (Some(old @ Array(_)), new @ Array(_)) => {
                        let mut arr = old.as_array().unwrap().clone();
                        arr.extend(new.as_array().unwrap().iter().cloned());
                        JsonValue::Array(arr)
                    }
                    (_, new) => new, // replace scalars / mismatched types
                };
                a.insert(k, v_merged);
            }
            JsonValue::Object(a)
        }
        // default: replace base with patch
        (_, b) => b,
    }
}

// ---------- Format-agnostic read-as-Value ----------
fn read_value(shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<JsonValue> {
    let p = path.as_ref();
    let error_context = || format!("Failed to parse config file {:?}.", p);
    match p.extension().and_then(|ext| ext.to_str()) {
        Some("yaml") | Some("yml") => {
            read_yaml_file::<JsonValue>(shell, &p).with_context(error_context)
        }
        Some("toml") => read_toml_file::<JsonValue>(shell, &p).with_context(error_context),
        Some("json") => read_json_file::<JsonValue>(shell, &p).with_context(error_context),
        _ => bail!(format!(
            "Unsupported file extension for config file {:?}.",
            p
        )),
    }
}
