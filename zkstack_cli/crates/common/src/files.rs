use std::path::{Path, PathBuf};

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

// Find file in all parents repository and return necessary path or an empty error if nothing has been found
pub fn find_file(shell: &Shell, path_buf: PathBuf, file_name: &str) -> anyhow::Result<PathBuf> {
    let _dir = shell.push_dir(path_buf);
    if shell.path_exists(file_name) {
        Ok(shell.current_dir())
    } else {
        let current_dir = shell.current_dir();
        let Some(path) = current_dir.parent() else {
            return Err(anyhow::anyhow!(
                "Failed to find file `{}` in the current directory or its parents",
                file_name
            ));
        };
        find_file(shell, path.to_path_buf(), file_name)
    }
}
