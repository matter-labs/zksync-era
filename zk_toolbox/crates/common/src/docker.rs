use std::collections::HashMap;

use url::Url;
use xshell::{cmd, Shell};

use crate::cmd::Cmd;

pub fn up(shell: &Shell, docker_compose_file: &str, detach: bool) -> anyhow::Result<()> {
    let args = if detach { vec!["-d"] } else { vec![] };
    let mut cmd = Cmd::new(cmd!(
        shell,
        "docker compose -f {docker_compose_file} up {args...}"
    ));
    cmd = if !detach { cmd.with_force_run() } else { cmd };
    Ok(cmd.run()?)
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

pub fn adjust_localhost_for_docker(mut url: Url) -> anyhow::Result<Url> {
    if let Some(host) = url.host_str() {
        if host == "localhost" || host == "127.0.0.1" {
            url.set_host(Some("host.docker.internal"))?;
        }
    } else {
        anyhow::bail!("Failed to parse: no host");
    }
    Ok(url)
}
