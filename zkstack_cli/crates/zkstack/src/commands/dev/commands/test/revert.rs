use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, logger};
use zkstack_cli_config::{EcosystemConfig, ZkStackConfig};

use super::{args::revert::RevertArgs, utils::install_and_build_dependencies};
use crate::commands::dev::messages::{MSG_REVERT_TEST_RUN_INFO, MSG_REVERT_TEST_RUN_SUCCESS};

const REVERT_TESTS_PATH: &str = "core/tests/revert-test";

pub async fn run(shell: &Shell, args: RevertArgs) -> anyhow::Result<()> {
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;
    shell.change_dir(ecosystem_config.link_to_code.join(REVERT_TESTS_PATH));

    logger::info(MSG_REVERT_TEST_RUN_INFO);

    if !args.no_deps {
        install_and_build_dependencies(shell, &ecosystem_config.link_to_code)?;
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
    let cmd = cmd!(shell, "yarn mocha tests/revert-and-restart-en.test.ts");
    let cmd = Cmd::new(cmd)
        .env("CHAIN_NAME", ecosystem_config.current_chain())
        .env("NO_KILL", args.no_kill.to_string());
    cmd.with_force_run().run()?;

    Ok(())
}
