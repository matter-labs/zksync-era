use std::path::PathBuf;

use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::logger;
use zkstack_cli_config::{traits::SaveConfig, ZkStackConfig};

use crate::commands::dev::{
    commands::test::utils::{TestWallets, TEST_WALLETS_PATH},
    messages::{
        MSG_CHAIN_NOT_FOUND_ERR, MSG_DESERIALIZE_TEST_WALLETS_ERR, MSG_INIT_TEST_WALLET_RUN_INFO,
        MSG_INIT_TEST_WALLET_RUN_SUCCESS,
    },
};

pub async fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;

    logger::info(MSG_INIT_TEST_WALLET_RUN_INFO);

    // Load test wallets configuration
    let wallets_path: PathBuf = ecosystem_config.link_to_code.join(TEST_WALLETS_PATH);
    let wallets: TestWallets = serde_json::from_str(shell.read_file(&wallets_path)?.as_ref())
        .context(MSG_DESERIALIZE_TEST_WALLETS_ERR)?;

    let mut chain_wallets = chain_config.get_wallets_config()?;
    let test_wallet = wallets.get_test_wallet(&chain_config)?;

    let wallets_path: PathBuf = ecosystem_config.link_to_code.join(TEST_WALLETS_PATH);
    let raw_wallets = shell.read_file(&wallets_path)?;
    let wallets: TestWallets =
        serde_json::from_str(&raw_wallets).context(MSG_DESERIALIZE_TEST_WALLETS_ERR)?;

    wallets
        .init_test_wallet(&ecosystem_config, &chain_config)
        .await?;

    chain_wallets.test_wallet = Some(test_wallet.clone());
    chain_wallets.save(shell, chain_config.configs.join("wallets.yaml"))?;

    logger::outro(MSG_INIT_TEST_WALLET_RUN_SUCCESS);

    Ok(())
}
