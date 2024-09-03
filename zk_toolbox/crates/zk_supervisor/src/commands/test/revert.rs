use std::path::PathBuf;

use anyhow::Context;
use common::{cmd::Cmd, config::global_config, logger, spinner::Spinner};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use super::{
    args::revert::RevertArgs,
    utils::{install_and_build_dependencies, TestWallets, TEST_WALLETS_PATH},
};
use crate::messages::{
    msg_revert_tests_run, MSG_CHAIN_NOT_FOUND_ERR, MSG_DESERIALIZE_TEST_WALLETS_ERR,
    MSG_REVERT_TEST_RUN_INFO, MSG_REVERT_TEST_RUN_SUCCESS,
};

const REVERT_TESTS_PATH: &str = "core/tests/revert-test";

pub async fn run(shell: &Shell, args: RevertArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    shell.change_dir(ecosystem_config.link_to_code.join(REVERT_TESTS_PATH));

    logger::info(MSG_REVERT_TEST_RUN_INFO);

    if !args.no_deps {
        install_and_build_dependencies(shell, &ecosystem_config)?;
    }

    run_test(shell, &args, &ecosystem_config).await?;
    logger::outro(MSG_REVERT_TEST_RUN_SUCCESS);

    Ok(())
}

async fn run_test(
    shell: &Shell,
    args: &RevertArgs,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<()> {
    Spinner::new(&msg_revert_tests_run(args.external_node)).freeze();

    let chain_config = ecosystem_config
        .load_chain(global_config().chain_name.clone())
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;

    let wallets_path: PathBuf = ecosystem_config.link_to_code.join(TEST_WALLETS_PATH);
    let wallets: TestWallets = serde_json::from_str(shell.read_file(&wallets_path)?.as_ref())
        .context(MSG_DESERIALIZE_TEST_WALLETS_ERR)?;

    wallets
        .init_test_wallet(ecosystem_config, &chain_config)
        .await?;

    let cmd = if args.external_node {
        cmd!(shell, "yarn mocha tests/revert-and-restart-en.test.ts")
    } else {
        cmd!(shell, "yarn mocha tests/revert-and-restart.test.ts")
    };

    let mut cmd = Cmd::new(cmd)
        .env("CHAIN_NAME", ecosystem_config.current_chain())
        .env("NO_KILL", args.no_kill.to_string())
        .env("MASTER_WALLET_PK", wallets.get_test_pk(&chain_config)?);
    if args.enable_consensus {
        cmd = cmd.env("ENABLE_CONSENSUS", "true");
    }
    cmd.with_force_run().run()?;

    Ok(())
}
