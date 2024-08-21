use std::path::PathBuf;

use anyhow::Context;
use common::{config::global_config, logger};
use config::EcosystemConfig;
use xshell::Shell;

use crate::messages::{
    MSG_DESERIALIZE_TEST_WALLETS_ERR, MSG_TEST_WALLETS_INFO, MSG_WALLETS_TEST_SUCCESS,
};

use super::utils::{TestWallets, TEST_WALLETS_PATH};

pub fn run(shell: &Shell) -> anyhow::Result<()> {
    logger::info(MSG_TEST_WALLETS_INFO);

    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let chain_config = ecosystem_config
        .load_chain(global_config().chain_name.clone())
        .context("Chain not found")?;

    let wallets_path: PathBuf = ecosystem_config.link_to_code.join(TEST_WALLETS_PATH);
    let wallets: TestWallets = serde_json::from_str(shell.read_file(&wallets_path)?.as_ref())
        .context(MSG_DESERIALIZE_TEST_WALLETS_ERR)?;

    logger::info(format!("Main: {:#?}", wallets.get_main_wallet()?));
    logger::info(format!(
        "Chain: {:#?}",
        wallets.get_test_wallet(&chain_config)?
    ));

    logger::outro(MSG_WALLETS_TEST_SUCCESS);

    Ok(())
}
