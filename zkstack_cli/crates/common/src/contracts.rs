use std::path::PathBuf;

use xshell::{cmd, Shell};

use crate::cmd::Cmd;

pub fn build_l1_contracts(shell: Shell, link_to_code: PathBuf) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code.join("contracts/l1-contracts"));
    Ok(Cmd::new(cmd!(shell, "yarn build:foundry")).run()?)
}

pub fn build_l1_da_contracts(shell: Shell, link_to_code: PathBuf) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code.join("contracts/da-contracts"));
    Ok(Cmd::new(cmd!(shell, "forge build")).run()?)
}

pub fn build_l2_contracts(shell: Shell, link_to_code: PathBuf) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code.join("contracts/l2-contracts"));
    // Ok(Cmd::new(cmd!(
    //     shell,
    //     "forge build --zksync --zk-enable-eravm-extensions"
    // ))
    // .run()?)
    Cmd::new(cmd!(shell, "yarn build:foundry")).run()?;
    Ok(())
}

pub fn build_system_contracts(shell: Shell, link_to_code: PathBuf) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code.join("contracts/system-contracts"));
    // Do not update era-contract's lockfile to avoid dirty submodule
    Cmd::new(cmd!(shell, "yarn install --frozen-lockfile")).run()?;
    Ok(Cmd::new(cmd!(shell, "yarn build:foundry")).run()?)
}
