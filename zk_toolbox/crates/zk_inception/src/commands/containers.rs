use anyhow::Context;
use common::{docker, logger, spinner::Spinner};
use xshell::Shell;

use crate::{configs::EcosystemConfig, consts::DOCKER_COMPOSE_FILE};

pub fn run(shell: &Shell) -> anyhow::Result<()> {
    EcosystemConfig::from_file(shell).context("Failed to find ecosystem folder.")?;
    logger::info("Starting containers");

    let spinner = Spinner::new("Starting containers using docker...");
    start_containers(shell)?;
    spinner.finish();

    logger::outro("Containers started successfully");
    Ok(())
}

pub fn start_containers(shell: &Shell) -> anyhow::Result<()> {
    docker::up(shell, DOCKER_COMPOSE_FILE).context("Failed to start containers")
}
