use std::path::Path;

use xshell::{cmd, Shell};

use crate::cmd::Cmd;

pub fn build_l1_contracts(shell: Shell, link_to_contracts: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_contracts.join("l1-contracts"));
    // Do not update era-contract's lockfile to avoid dirty submodule
    // Note, tha the v26 contracts depend on the node_modules to be present at the time of the compilation.
    Cmd::new(cmd!(shell, "yarn install --frozen-lockfile")).run()?;
    Ok(Cmd::new(cmd!(shell, "yarn build:foundry")).run()?)
}

pub fn build_l1_da_contracts(shell: Shell, link_to_contracts: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_contracts.join("da-contracts"));
    Ok(Cmd::new(cmd!(shell, "forge build")).run()?)
}

pub fn build_l2_contracts(shell: Shell, link_to_contracts: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_contracts.join("l2-contracts"));
    Cmd::new(cmd!(shell, "yarn build:foundry")).run()?;
    Ok(())
}

pub fn build_system_contracts(shell: Shell, link_to_contracts: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_contracts.join("system-contracts"));
    Ok(Cmd::new(cmd!(shell, "yarn build:foundry")).run()?)
}
