use std::collections::HashMap;

use xshell::{cmd, Shell};

use crate::cmd::Cmd;

pub fn up(shell: &Shell, docker_compose_file: &str) -> anyhow::Result<()> {
    Ok(Cmd::new(cmd!(shell, "docker compose -f {docker_compose_file} up -d")).run()?)
}

pub fn down(shell: &Shell, docker_compose_file: &str) -> anyhow::Result<()> {
    Ok(Cmd::new(cmd!(shell, "docker compose -f {docker_compose_file} down")).run()?)
}

pub fn run(
    shell: &Shell,
    docker_image: &str,
    docker_args: HashMap<String, String>,
) -> anyhow::Result<()> {
    let mut args = vec![];
    for (key, value) in docker_args.iter() {
        args.push(key);
        args.push(value);
    }
    Ok(Cmd::new(cmd!(shell, "docker run {args...} {docker_image}")).run()?)
}
