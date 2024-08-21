use std::path::PathBuf;

use anyhow::Context;
use common::{cmd::Cmd, config::global_config, logger, server::Server, spinner::Spinner};
use config::EcosystemConfig;
use ethers::utils::hex::ToHex;
use xshell::{cmd, Shell};

use super::{
    args::recovery::RecoveryArgs,
    utils::{install_and_build_dependencies, TestWallets, TEST_WALLETS_PATH},
};
use crate::messages::{
    MSG_CHAIN_NOT_FOUND_ERR, MSG_RECOVERY_TEST_RUN_INFO, MSG_RECOVERY_TEST_RUN_SUCCESS,
};

const RECOVERY_TESTS_PATH: &str = "core/tests/recovery-test";

pub async fn run(shell: &Shell, args: RecoveryArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    shell.change_dir(ecosystem_config.link_to_code.join(RECOVERY_TESTS_PATH));

    logger::info(MSG_RECOVERY_TEST_RUN_INFO);
    Server::new(None, ecosystem_config.link_to_code.clone()).build(shell)?;

    if !args.no_deps {
        install_and_build_dependencies(shell, &ecosystem_config)?;
    }

    run_test(shell, &args, &ecosystem_config).await?;
    logger::outro(MSG_RECOVERY_TEST_RUN_SUCCESS);

    Ok(())
}

async fn run_test(
    shell: &Shell,
    args: &RecoveryArgs,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<()> {
    Spinner::new("Running test...").freeze();
    let chain_config = ecosystem_config
        .load_chain(global_config().chain_name.clone())
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;

    let cmd = if args.snapshot {
        cmd!(shell, "yarn mocha tests/snapshot-recovery.test.ts")
    } else {
        cmd!(shell, "yarn mocha tests/genesis-recovery.test.ts")
    };

    let wallets_path: PathBuf = ecosystem_config.link_to_code.join(TEST_WALLETS_PATH);
    let wallets: TestWallets = serde_json::from_str(shell.read_file(&wallets_path)?.as_ref())
        .context("Impossible to deserialize test wallets")?;

    wallets
        .init_test_wallet(ecosystem_config, &chain_config)
        .await?;

    let private_key = wallets.get_test_wallet(&chain_config)?.private_key.unwrap();

    let cmd = Cmd::new(cmd)
        .env("CHAIN_NAME", ecosystem_config.current_chain())
        .env("MASTER_WALLET_PK", private_key.encode_hex::<String>());

    cmd.with_force_run().run()?;

    Ok(())
}
