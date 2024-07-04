use anyhow::Context;
use clap::Subcommand;
use common::{docker, logger};
use config::{EcosystemConfig, DOCKER_COMPOSE_FILE};
use xshell::Shell;

#[derive(Subcommand, Debug)]
pub enum CleanCommands {
    All,
    Containers,
    ContractsCache,
}

pub fn run(shell: &Shell, args: CleanCommands) -> anyhow::Result<()> {
    let ecosystem = EcosystemConfig::from_file(shell)?;
    match args {
        CleanCommands::All => {
            containers(shell)?;
            contracts(shell, &ecosystem)?;
        }
        CleanCommands::Containers => containers(shell)?,
        CleanCommands::ContractsCache => contracts(&shell, &ecosystem)?,
    }
    Ok(())
}

pub fn containers(shell: &Shell) -> anyhow::Result<()> {
    logger::info("Docker down");
    docker::down(shell, DOCKER_COMPOSE_FILE)?;
    logger::info("Docker remove volumes");
    shell.remove_path("volumes")?;
    logger::info("Docker containers cleaned");
    Ok(())
}

pub fn contracts(shell: &Shell, ecosystem_config: &EcosystemConfig) -> anyhow::Result<()> {
    let path_to_foundry = ecosystem_config.path_to_foundry();
    logger::info("Cleaning contracts");
    shell
        .remove_path(path_to_foundry.join("artifacts"))
        .context("artifacts")?;
    shell
        .remove_path(path_to_foundry.join("cache"))
        .context("cache")?;
    shell
        .remove_path(path_to_foundry.join("cache-forge"))
        .context("cache-forge")?;
    shell
        .remove_path(path_to_foundry.join("out"))
        .context("out")?;
    shell
        .remove_path(path_to_foundry.join("typechain"))
        .context("typechain")?;
    shell
        .remove_path(path_to_foundry.join("script-config"))
        .context("remove script-config")?;
    shell
        .create_dir(path_to_foundry.join("script-config"))
        .context("create script-config")?;
    shell
        .remove_path(path_to_foundry.join("script-out"))
        .context("remove script-out")?;
    shell
        .create_dir(path_to_foundry.join("script-out"))
        .context("create script-out")?;
    logger::info("Contracts cleaning successful");
    Ok(())
}
