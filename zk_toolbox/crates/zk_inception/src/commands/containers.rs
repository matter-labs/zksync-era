use std::path::PathBuf;

use anyhow::{anyhow, Context};
use common::{docker, logger, spinner::Spinner};
use config::{EcosystemConfig, DOCKER_COMPOSE_FILE, ERA_OBSERVABILITY_COMPOSE_FILE};
use xshell::Shell;

use crate::messages::{
    MSG_CONTAINERS_STARTED, MSG_FAILED_TO_FIND_ECOSYSTEM_ERR, MSG_RETRY_START_CONTAINERS_PROMPT,
    MSG_STARTING_CONTAINERS, MSG_STARTING_DOCKER_CONTAINERS_SPINNER,
};

use super::args::ContainersArgs;

pub fn run(shell: &Shell, args: ContainersArgs) -> anyhow::Result<()> {
    let ecosystem = EcosystemConfig::from_file(shell).context(MSG_FAILED_TO_FIND_ECOSYSTEM_ERR)?;

    initialize_docker(shell, &ecosystem)?;

    logger::info(MSG_STARTING_CONTAINERS);

    let spinner = Spinner::new(MSG_STARTING_DOCKER_CONTAINERS_SPINNER);
    start_containers(shell, args.run_observability)?;
    spinner.finish();

    logger::outro(MSG_CONTAINERS_STARTED);
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

fn start_container(shell: &Shell, compose_file: &str, retry_msg: &str) -> anyhow::Result<()> {
    while let Err(err) = docker::up(shell, compose_file) {
        logger::error(err.to_string());
        if !common::PromptConfirm::new(retry_msg).default(true).ask() {
            return Err(err);
        }
    }
    Ok(())
}

pub fn start_containers(shell: &Shell, run_observability: bool) -> anyhow::Result<()> {
    start_container(
        shell,
        DOCKER_COMPOSE_FILE,
        MSG_RETRY_START_CONTAINERS_PROMPT,
    )?;

    if !run_observability {
        return Ok(());
    }
    start_container(
        shell,
        ERA_OBSERVABILITY_COMPOSE_FILE,
        MSG_RETRY_START_CONTAINERS_PROMPT,
    )?;

    Ok(())
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
    // For some reasons our docker-compose sometimes required .env file while we are investigating this behaviour
    // it's better to create file and don't make the life of customers harder
    shell.write_file(".env", "")?;
    Ok(())
}
