use std::{
    fs::{self, File},
    io::{self, Write},
    path::Path,
};

use serde::Serialize;
use xshell::Shell;

pub fn save_yaml_file(
    shell: &Shell,
    file_path: impl AsRef<Path>,
    content: impl Serialize,
    comment: impl ToString,
) -> anyhow::Result<()> {
    let data = format!(
        "{}/n{}",
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
    let data = format!("{}/n{}", comment.to_string(), toml::to_string(&content)?);
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
