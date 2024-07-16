use xshell::{cmd, Shell};

use crate::cmd::Cmd;

pub fn up(shell: &Shell, docker_compose_file: &str) -> anyhow::Result<()> {
    Ok(Cmd::new(cmd!(shell, "docker compose -f {docker_compose_file} up -d")).run()?)
}

pub fn down(shell: &Shell, docker_compose_file: &str) -> anyhow::Result<()> {
    Ok(Cmd::new(cmd!(shell, "docker compose -f {docker_compose_file} down")).run()?)
}
