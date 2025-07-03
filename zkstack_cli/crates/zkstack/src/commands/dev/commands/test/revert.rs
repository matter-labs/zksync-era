use std::path::PathBuf;

use anyhow::Context;
use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, logger};
use zkstack_cli_config::EcosystemConfig;

use super::{
    args::revert::RevertArgs,
    utils::{install_and_build_dependencies, TestWallets, TEST_WALLETS_PATH},
};
use crate::commands::dev::messages::{
    MSG_CHAIN_NOT_FOUND_ERR, MSG_DESERIALIZE_TEST_WALLETS_ERR, MSG_REVERT_TEST_RUN_INFO,
    MSG_REVERT_TEST_RUN_SUCCESS,
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
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;

    let wallets_path: PathBuf = ecosystem_config.link_to_code.join(TEST_WALLETS_PATH);
    let wallets: TestWallets = serde_json::from_str(shell.read_file(&wallets_path)?.as_ref())
        .context(MSG_DESERIALIZE_TEST_WALLETS_ERR)?;

    wallets
        .init_test_wallet(ecosystem_config, &chain_config)
        .await?;

    let cmd = cmd!(shell, "yarn mocha tests/revert-and-restart-en.test.ts");
    let cmd = Cmd::new(cmd)
        .env("CHAIN_NAME", ecosystem_config.current_chain())
        .env("NO_KILL", args.no_kill.to_string());
    cmd.with_force_run().run()?;

    Ok(())
}
