use std::path::Path;

use serde::{de::DeserializeOwned, Serialize};
use xshell::Shell;

pub fn read_yaml_file<T>(shell: &Shell, file_path: impl AsRef<Path>) -> anyhow::Result<T>
where
    T: DeserializeOwned,
{
    let content = shell.read_file(file_path)?;
    let yaml = serde_yaml::from_str(&content)?;
    Ok(yaml)
}

pub fn read_toml_file<T>(shell: &Shell, file_path: impl AsRef<Path>) -> anyhow::Result<T>
where
    T: DeserializeOwned,
{
    let content = shell.read_file(file_path)?;
    let toml = toml::from_str(&content)?;
    Ok(toml)
}

pub fn read_json_file<T>(shell: &Shell, file_path: impl AsRef<Path>) -> anyhow::Result<T>
where
    T: DeserializeOwned,
{
    let content = shell.read_file(file_path)?;
    let json = serde_json::from_str(&content)?;
    Ok(json)
}

pub fn save_yaml_file(
    shell: &Shell,
    file_path: impl AsRef<Path>,
    content: impl Serialize,
    comment: impl ToString,
) -> anyhow::Result<()> {
    let data = format!(
        "{}{}",
        comment.to_string(),
        serde_yaml::to_string(&content)?
    );
    shell.write_file(file_path, data)?;
    Ok(())
}

pub fn save_toml_file(
    shell: &Shell,
    file_path: impl AsRef<Path>,
    content: impl Serialize,
    comment: impl ToString,
) -> anyhow::Result<()> {
    let data = format!("{}{}", comment.to_string(), toml::to_string(&content)?);
    shell.write_file(file_path, data)?;
    Ok(())
}

pub fn save_json_file(
    shell: &Shell,
    file_path: impl AsRef<Path>,
    content: impl Serialize,
) -> anyhow::Result<()> {
    let data = serde_json::to_string_pretty(&content)?;
    shell.write_file(file_path, data)?;
    Ok(())
}
