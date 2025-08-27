use std::path::PathBuf;

use anyhow::Context;
use clap::Subcommand;
use xshell::Shell;
use zkstack_cli_common::{docker, logger};
use zkstack_cli_config::{ZkStackConfig, DOCKER_COMPOSE_FILE};

use crate::commands::dev::messages::{
    MSG_CONTRACTS_CLEANING, MSG_CONTRACTS_CLEANING_FINISHED, MSG_DOCKER_COMPOSE_DOWN,
};

#[derive(Subcommand, Debug)]
pub enum CleanCommands {
    /// Remove containers and contracts cache
    All,
    /// Remove containers and docker volumes
    Containers,
    /// Remove contracts caches
    ContractsCache,
}

pub fn run(shell: &Shell, args: CleanCommands) -> anyhow::Result<()> {
    let config = ZkStackConfig::from_file(shell)?;
    let path_to_foundry = config.path_to_l1_foundry();
    let link_to_code = config.link_to_code();
    match args {
        CleanCommands::All => {
            containers(shell)?;
            contracts(shell, path_to_foundry, link_to_code)?;
        }
        CleanCommands::Containers => containers(shell)?,
        CleanCommands::ContractsCache => contracts(shell, path_to_foundry, link_to_code)?,
    }
    Ok(())
}

pub fn containers(shell: &Shell) -> anyhow::Result<()> {
    logger::info(MSG_DOCKER_COMPOSE_DOWN);
    docker::down(shell, DOCKER_COMPOSE_FILE)?;
    Ok(())
}

pub fn contracts(
    shell: &Shell,
    path_to_foundry: PathBuf,
    contracts_path: PathBuf,
) -> anyhow::Result<()> {
    logger::info(MSG_CONTRACTS_CLEANING);
    shell
        .remove_path(path_to_foundry.join("broadcast"))
        .context("broadcast")?;
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
        .remove_path(path_to_foundry.join("zkout"))
        .context("zkout")?;
    shell
        .remove_path(path_to_foundry.join("typechain"))
        .context("typechain")?;
    shell
        .remove_path(contracts_path.join("da-contracts/cache-forge"))
        .context("l2-contracts/cache-forge")?;
    shell
        .remove_path(contracts_path.join("da-contracts/out"))
        .context("l2-contracts/out")?;
    shell
        .remove_path(contracts_path.join("da-contracts/zkout"))
        .context("l2-contracts/zkout")?;
    shell
        .remove_path(contracts_path.join("l2-contracts/cache-forge"))
        .context("l2-contracts/cache-forge")?;
    shell
        .remove_path(contracts_path.join("l2-contracts/zkout"))
        .context("l2-contracts/zkout")?;
    shell
        .remove_path(contracts_path.join("system-contracts/cache-forge"))
        .context("system-contracts/cache-forge")?;
    shell
        .remove_path(contracts_path.join("system-contracts/zkout"))
        .context("system-contracts/zkout")?;
    shell
        .remove_path(contracts_path.join("system-contracts/contracts-preprocessed"))
        .context("system-contracts/contracts-preprocessed")?;
    shell
        .remove_path(path_to_foundry.join("script-config"))
        .context("remove script-config")?;
    shell
        .create_dir(path_to_foundry.join("script-config"))
        .context("create script-config")?;
    shell.write_file(path_to_foundry.join("script-config/.gitkeep"), "")?;
    shell
        .remove_path(path_to_foundry.join("script-out"))
        .context("remove script-out")?;
    shell
        .create_dir(path_to_foundry.join("script-out"))
        .context("create script-out")?;
    shell.write_file(path_to_foundry.join("script-out/.gitkeep"), "")?;
    logger::info(MSG_CONTRACTS_CLEANING_FINISHED);
    Ok(())
}
