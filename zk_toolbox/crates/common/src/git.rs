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

pub fn pull(shell: &Shell, link_to_code: PathBuf) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code);
    let res = Cmd::new(cmd!(shell, "git rev-parse --abbrev-ref HEAD")).run_with_output()?;
    let current_branch = String::from_utf8(res.stdout)?;
    let current_branch = current_branch.trim_end();
    Cmd::new(cmd!(shell, "git pull origin {current_branch}")).run()?;
    Ok(())
}
