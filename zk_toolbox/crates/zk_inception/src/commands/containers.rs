use anyhow::{anyhow, Context};
use common::{docker, logger, spinner::Spinner};
use std::path::PathBuf;
use xshell::Shell;

use crate::{configs::EcosystemConfig, consts::DOCKER_COMPOSE_FILE};

pub fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem =
        EcosystemConfig::from_file(shell).context("Failed to find ecosystem folder.")?;

    initialize_docker(shell, &ecosystem)?;

    logger::info("Starting containers");

    let spinner = Spinner::new("Starting containers using docker...");
    start_containers(shell)?;
    spinner.finish();

    logger::outro("Containers started successfully");
    Ok(())
}

pub fn initialize_docker(shell: &Shell, ecosystem: &EcosystemConfig) -> anyhow::Result<()> {
    if !shell.path_exists("volumes") {
        create_docker_folders(shell)?;
    };

    if !shell.path_exists(DOCKER_COMPOSE_FILE) {
        copy_dockerfile(shell, ecosystem.link_to_code.clone())?;
    };

    Ok(())
}

pub fn start_containers(shell: &Shell) -> anyhow::Result<()> {
    docker::up(shell, DOCKER_COMPOSE_FILE).context("Failed to start containers")
}

fn create_docker_folders(shell: &Shell) -> anyhow::Result<()> {
    shell.create_dir("volumes")?;
    shell.create_dir("volumes/postgres")?;
    shell.create_dir("volumes/reth")?;
    shell.create_dir("volumes/reth/data")?;
    Ok(())
}

fn copy_dockerfile(shell: &Shell, link_to_code: PathBuf) -> anyhow::Result<()> {
    let docker_compose_file = link_to_code.join(DOCKER_COMPOSE_FILE);

    let docker_compose_text = shell.read_file(&docker_compose_file).map_err(|err| {
        anyhow!(
            "Failed to read docker compose file from {:?}: {}",
            &docker_compose_file,
            err
        )
    })?;
    let original_source = "./etc/reth/chaindata";
    let new_source = link_to_code.join(original_source);
    let new_source = new_source.to_str().unwrap();

    let data = docker_compose_text.replace(original_source, new_source);
    shell.write_file(DOCKER_COMPOSE_FILE, data)?;
    Ok(())
}
