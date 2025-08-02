use std::path::PathBuf;

use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::logger;
use zkstack_cli_config::ZkStackConfig;

use super::utils::{TestWallets, TEST_WALLETS_PATH};
use crate::commands::dev::messages::{
    MSG_DESERIALIZE_TEST_WALLETS_ERR, MSG_TEST_WALLETS_INFO, MSG_WALLETS_TEST_SUCCESS,
};

pub fn run(shell: &Shell) -> anyhow::Result<()> {
    logger::info(MSG_TEST_WALLETS_INFO);

    let config = ZkStackConfig::current_chain(shell)?;

    let wallets_path: PathBuf = config.link_to_code.join(TEST_WALLETS_PATH);
    let wallets: TestWallets = serde_json::from_str(shell.read_file(wallets_path)?.as_ref())
        .context(MSG_DESERIALIZE_TEST_WALLETS_ERR)?;

    logger::info(format!("Main: {:#?}", wallets.get_main_wallet()?));
    logger::info(format!("Chain: {:#?}", wallets.get_test_wallet(&config)?));

    logger::outro(MSG_WALLETS_TEST_SUCCESS);

    Ok(())
}
