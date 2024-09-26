use std::path::Path;

use xshell::{cmd, Shell};

use crate::cmd::Cmd;

pub fn build_l2_contracts(shell: &Shell, link_to_code: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code.join("contracts"));
    Ok(Cmd::new(cmd!(shell, "yarn l2 build")).run()?)
}

/// Builds L1 contracts using hardhat. This is a temporary measure, mainly needed to
/// compile the contracts with zksolc (for some reason doing it via foundry took too much time).
pub fn build_l1_contracts(shell: &Shell, link_to_code: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code.join("contracts"));
    Ok(Cmd::new(cmd!(shell, "yarn l1 build")).run()?)
}
