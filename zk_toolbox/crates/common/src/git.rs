use std::path::PathBuf;

use xshell::{cmd, Shell};

use crate::cmd::Cmd;

pub fn clone(
    shell: &Shell,
    path: PathBuf,
    repository: &str,
    name: &str,
) -> anyhow::Result<PathBuf> {
    let _dir = shell.push_dir(path);
    Cmd::new(cmd!(
        shell,
        "git clone --recurse-submodules {repository} {name}"
    ))
    .run()?;
    Ok(shell.current_dir().join(name))
}

pub fn submodule_update(shell: &Shell, link_to_code: PathBuf) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code);
    Cmd::new(cmd!(
        shell,
        "git submodule update --init --recursive
"
    ))
    .run()?;
    Ok(())
}
