use common::{docker, logger, spinner::Spinner};
use config::DOCKER_COMPOSE_FILE;
use xshell::Shell;

use crate::messages::{
    MSG_CONTAINERS_STOPPED, MSG_STOPPING_CONTAINERS, MSG_STOPPING_DOCKER_CONTAINERS_SPINNER,
};

pub fn run(shell: &Shell) -> anyhow::Result<()> {
    logger::info(MSG_STOPPING_CONTAINERS);
    docker_down(shell)?;
    logger::outro(MSG_CONTAINERS_STOPPED);
    Ok(())
}

pub fn docker_down(shell: &Shell) -> anyhow::Result<()> {
    let spinner = Spinner::new(MSG_STOPPING_DOCKER_CONTAINERS_SPINNER);
    docker::down(shell, DOCKER_COMPOSE_FILE)?;
    spinner.finish();
    Ok(())
}
