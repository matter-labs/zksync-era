use config::zkstack_config::ZkStackConfig;
use xshell::Shell;

use super::utils::{build_contracts, install_and_build_dependencies};

pub fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;

    install_and_build_dependencies(shell, &ecosystem_config)?;
    build_contracts(shell, &ecosystem_config)?;

    Ok(())
}
