use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, logger};
use zkstack_cli_config::ZkStackConfig;

use crate::commands::dev::messages::MSG_L1_CONTRACTS_TEST_SUCCESS;

pub fn run(shell: &Shell) -> anyhow::Result<()> {
    let config = ZkStackConfig::from_file(shell)?;
    let _dir_guard = shell.push_dir(config.link_to_code());

    Cmd::new(cmd!(shell, "yarn l1-contracts test"))
        .with_force_run()
        .run()?;

    logger::outro(MSG_L1_CONTRACTS_TEST_SUCCESS);

    Ok(())
}
