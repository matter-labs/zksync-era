use anyhow::Context;
use common::logger;
use config::{EcosystemConfig, DOCKER_COMPOSE_FILE};
use xshell::Shell;

use crate::messages::{
    MSG_CONTAINERS_RESETTED, MSG_FAILED_TO_FIND_ECOSYSTEM_ERR, MSG_RESETTING_CONTAINERS,
};

pub fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem = EcosystemConfig::from_file(shell).context(MSG_FAILED_TO_FIND_ECOSYSTEM_ERR)?;

    logger::info(MSG_RESETTING_CONTAINERS);

    super::down::docker_down(shell)?;
    remove_docker(shell, &ecosystem)?;
    super::up::docker_up(shell, &ecosystem)?;

    logger::outro(MSG_CONTAINERS_RESETTED);
    Ok(())
}

fn remove_docker(shell: &Shell, ecosystem: &EcosystemConfig) -> anyhow::Result<()> {
    if shell.path_exists("volumes") {
        shell.remove_path("volumes")?;
    };

    if shell.path_exists(DOCKER_COMPOSE_FILE)
        && ecosystem.link_to_code.join(DOCKER_COMPOSE_FILE)
            != shell.current_dir().join(DOCKER_COMPOSE_FILE)
    {
        shell.remove_path(DOCKER_COMPOSE_FILE)?;
    };

    Ok(())
}
