use std::path::PathBuf;

use xshell::{cmd, Shell};

use crate::cmd::Cmd;

pub fn build_test_contracts(shell: Shell, link_to_code: PathBuf) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code.join("etc/contracts-test-data"));
    Cmd::new(cmd!(shell, "yarn install")).run()?;
    Ok(Cmd::new(cmd!(shell, "yarn build")).run()?)
}

pub fn build_l1_contracts(shell: Shell, link_to_code: PathBuf) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code.join("contracts/l1-contracts"));
    Ok(Cmd::new(cmd!(shell, "forge build")).run()?)
}

pub fn build_l2_contracts(shell: Shell, link_to_code: PathBuf) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code.join("contracts/l2-contracts"));
    Ok(Cmd::new(cmd!(
        shell,
        "forge build --zksync --zk-enable-eravm-extensions"
    ))
    .run()?)
}

pub fn build_system_contracts(shell: Shell, link_to_code: PathBuf) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code.join("contracts/system-contracts"));
    Cmd::new(cmd!(shell, "yarn install")).run()?;
    Cmd::new(cmd!(shell, "yarn preprocess:system-contracts")).run()?;
    Cmd::new(cmd!(
        shell,
        "forge build --zksync --zk-enable-eravm-extensions"
    ))
    .run()?;
    Cmd::new(cmd!(shell, "yarn preprocess:bootloader")).run()?;
    Ok(Cmd::new(cmd!(
        shell,
        "forge build --zksync --zk-enable-eravm-extensions"
    ))
    .run()?)
}
