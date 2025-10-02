use std::path::Path;

use xshell::{cmd, Shell};
use zkstack_cli_common::cmd::Cmd;

pub(super) fn install_yarn_dependencies(shell: &Shell, link_to_code: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code);
    Ok(Cmd::new(cmd!(shell, "yarn install")).run()?)
}

pub(super) fn build_system_contracts(
    shell: &Shell,
    link_to_contracts: &Path,
) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_contracts);
    Ok(Cmd::new(cmd!(shell, "yarn sc build")).run()?)
}

pub(super) fn build_da_contracts(shell: &Shell, link_to_contracts: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_contracts);
    Ok(Cmd::new(cmd!(shell, "yarn da build:foundry")).run()?)
}
