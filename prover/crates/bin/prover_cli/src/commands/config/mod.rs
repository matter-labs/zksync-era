use std::str::FromStr;

use clap::Subcommand;
use dialoguer::{theme::ColorfulTheme, Confirm, Input};

use crate::{cli::ProverCLIConfig, helper::config_path};

pub mod utils;

#[derive(Subcommand)]
pub enum ConfigCommand {
    Set,
    Edit,
    Display,
}

impl ConfigCommand {
    pub(crate) async fn run(self) -> anyhow::Result<()> {
        match self {
            ConfigCommand::Set => {
                let config_path = config_path()?;
                let config = prompt_config()?;
                let toml_config = toml::to_string_pretty(&config)?;
                println!("Config saved at: {}\n{toml_config}", config_path.display());
                std::fs::write(config_path, toml_config)?;
                Ok(())
            }
            ConfigCommand::Edit => {
                println!("EDIT");
                Ok(())
            }
            ConfigCommand::Display => {
                println!("DISPLAY");
                Ok(())
            }
        }
    }
}

pub fn prompt_config() -> anyhow::Result<ProverCLIConfig> {
    let prompted_config = ProverCLIConfig {
        db_url: prompt(
            "PROVER DATABASE URL",
            "postgres://postgres:notsecurepassword@localhost/prover_local".into(),
        )
        .ok(),
    };
    Ok(prompted_config)
}

pub fn confirm(prompt: &str) -> anyhow::Result<bool> {
    Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .show_default(true)
        .default(false)
        .interact()
        .map_err(Into::into)
}

pub fn prompt<T>(prompt: &str, default: T) -> anyhow::Result<T>
where
    T: Clone + ToString + FromStr,
    <T as FromStr>::Err: ToString,
{
    Input::<T>::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .default(default)
        .show_default(true)
        .interact_text()
        .map_err(Into::into)
}

// pub async fn run(cfg: ProverCLIConfig) -> anyhow::Result<()> {
//     let path = get_envfile()?;
//     println!("{:?}", path);
//     let res = load_envfile(&path);
//     println!("{:?}", res);
//     let res2 = update_envfile(&path, "PLI__CONFIG", "asd");
//     println!("{:?}", res2);
//     let res = load_envfile(&path);
//     println!("{:?}", res);
//     Ok(())
// }
