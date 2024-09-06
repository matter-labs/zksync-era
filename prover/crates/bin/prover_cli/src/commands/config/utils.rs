use std::{io::Write, path::PathBuf};

use anyhow::Context;

use crate::{cli::ProverCLIConfig, commands, helper::config_path};

pub async fn load_config() -> anyhow::Result<ProverCLIConfig> {
    let config_path = config_path()?;
    if !config_path.exists() {
        println!("No config set.");
        commands::config::ConfigCommand::Set.run().await?;
    }
    let config = std::fs::read_to_string(config_path).context("Failed to read config file")?;
    toml::from_str(&config).context("Failed to parse config file")
}

pub fn get_envfile() -> anyhow::Result<PathBuf> {
    if let Ok(envfile) = std::env::var("PLI__CONFIG") {
        return Ok(envfile.into());
    }
    Ok(config_path()?)
}

pub fn load_envfile(path: impl AsRef<std::path::Path>) -> anyhow::Result<()> {
    std::fs::read_to_string(path)?
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .for_each(|(k, v)| std::env::set_var(k, v));
    Ok(())
}

pub fn update_envfile(
    path: impl AsRef<std::path::Path> + std::marker::Copy,
    key: impl AsRef<str>,
    value: impl AsRef<str>,
) -> anyhow::Result<()> {
    let prefix = format!("{}=", key.as_ref());
    let kv = format!("{}={}", key.as_ref(), value.as_ref());
    let swapfile = path.as_ref().with_extension(".swp");
    let mut out = std::io::BufWriter::new(std::fs::File::create(&swapfile)?);
    let mut found = false;

    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|l| {
            if l.starts_with(&prefix) {
                found = true;
                kv.clone()
            } else {
                l.to_string()
            }
        })
        .try_for_each(|l| writeln!(&mut out, "{}", l))?;
    if !found {
        writeln!(&mut out, "{}", kv)?;
    }
    out.flush()?;
    std::fs::rename(swapfile, path)?;

    Ok(())
}
