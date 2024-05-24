use std::path::Path;

use anyhow::{bail, Context};
use common::files::{save_json_file, save_toml_file, save_yaml_file};
use serde::{de::DeserializeOwned, Serialize};
use xshell::Shell;

/// Reads a config file from a given path, correctly parsing file extension.
/// Supported file extensions are: `yaml`, `yml`, `toml`, `json`.
pub trait ReadConfig: DeserializeOwned + Clone {
    fn read(shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let file = shell.read_file(&path).with_context(|| {
            format!(
                "Failed to open config file. Please check if the file exists: {:?}",
                path.as_ref()
            )
        })?;
        let error_context = || format!("Failed to parse config file {:?}.", path.as_ref());

        match path.as_ref().extension().and_then(|ext| ext.to_str()) {
            Some("yaml") | Some("yml") => serde_yaml::from_str(&file).with_context(error_context),
            Some("toml") => toml::from_str(&file).with_context(error_context),
            Some("json") => serde_json::from_str(&file).with_context(error_context),
            _ => bail!(format!(
                "Unsupported file extension for config file {:?}.",
                path.as_ref()
            )),
        }
    }
}

/// Saves a config file to a given path, correctly parsing file extension.
/// Supported file extensions are: `yaml`, `yml`, `toml`, `json`.
pub trait SaveConfig: Serialize + Sized {
    fn save(&self, shell: &Shell, path: impl AsRef<Path>) -> anyhow::Result<()> {
        save_with_comment(shell, path, self, "")
    }
}

/// Saves a config file to a given path, correctly parsing file extension.
/// Supported file extensions are: `yaml`, `yml`, `toml`.
pub trait SaveConfigWithComment: Serialize + Sized {
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
