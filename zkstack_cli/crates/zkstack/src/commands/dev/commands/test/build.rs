use xshell::Shell;
use zkstack_cli_config::{ZkStackConfig, ZkStackConfigTrait};

use super::utils::{build_contracts, install_and_build_dependencies};

pub fn run(shell: &Shell) -> anyhow::Result<()> {
    let link_to_code = ZkStackConfig::from_file(shell)?.link_to_code();

    install_and_build_dependencies(shell, &link_to_code)?;
    build_contracts(shell, &link_to_code)?;

    Ok(())
}
