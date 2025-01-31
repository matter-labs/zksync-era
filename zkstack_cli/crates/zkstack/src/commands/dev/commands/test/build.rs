use xshell::Shell;
use zkstack_cli_config::EcosystemConfig;

use super::utils::{build_contracts, install_and_build_dependencies};

pub fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    install_and_build_dependencies(shell, &ecosystem_config)?;
    build_contracts(shell, &ecosystem_config)?;

    Ok(())
}
