use std::path::PathBuf;

use anyhow::Context;
use common::{cmd::Cmd, config::global_config, logger};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use super::{
    args::fees::FeesArgs,
    utils::{build_contracts, install_and_build_dependencies, TS_INTEGRATION_PATH},
};
use crate::{
    commands::test::utils::{TestWallets, TEST_WALLETS_PATH},
    messages::{
        MSG_CHAIN_NOT_FOUND_ERR, MSG_DESERIALIZE_TEST_WALLETS_ERR,
        MSG_INTEGRATION_TESTS_RUN_SUCCESS,
    },
};

pub async fn run(shell: &Shell, args: FeesArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    shell.change_dir(ecosystem_config.link_to_code.join(TS_INTEGRATION_PATH));
    let chain_config = ecosystem_config
        .load_current_chain()
        .expect(MSG_CHAIN_NOT_FOUND_ERR);

    if !args.no_deps {
        logger::info("Installing dependencies");
        build_contracts(shell, &ecosystem_config)?;
        install_and_build_dependencies(shell, &ecosystem_config)?;
    }

    logger::info(format!(
        "Running fees tests on chain: {}",
        ecosystem_config.current_chain()
    ));

    let wallets_path: PathBuf = ecosystem_config.link_to_code.join(TEST_WALLETS_PATH);
    let wallets: TestWallets = serde_json::from_str(shell.read_file(&wallets_path)?.as_ref())
        .context(MSG_DESERIALIZE_TEST_WALLETS_ERR)?;

    wallets
        .init_test_wallet(&ecosystem_config, &chain_config)
        .await?;

    let mut command = cmd!(shell, "yarn jest fees.test.ts --testTimeout 240000")
        .env("SPAWN_NODE", "1")
        .env("RUN_FEE_TEST", "1")
        .env("NO_KILL", args.no_kill.to_string())
        .env("CHAIN_NAME", ecosystem_config.current_chain())
        .env("MASTER_WALLET_PK", wallets.get_test_pk(&chain_config)?);

    if global_config().verbose {
        command = command.env(
            "ZKSYNC_DEBUG_LOGS",
            format!("{:?}", global_config().verbose),
        )
    }

    Cmd::new(command).with_force_run().run()?;

    logger::outro(MSG_INTEGRATION_TESTS_RUN_SUCCESS);

    Ok(())
}
