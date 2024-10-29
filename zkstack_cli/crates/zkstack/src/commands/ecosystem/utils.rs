use std::path::Path;

use common::cmd::Cmd;
use xshell::{cmd, Shell};

pub(super) fn install_yarn_dependencies(shell: &Shell, link_to_code: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code);
    Ok(Cmd::new(cmd!(shell, "yarn install")).run()?)
}

pub(super) fn build_system_contracts(shell: &Shell, link_to_code: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code.join("contracts"));
    Ok(Cmd::new(cmd!(shell, "yarn sc build")).run()?)
}
