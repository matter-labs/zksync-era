use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, logger, server::Server, spinner::Spinner};
use zkstack_cli_config::{EcosystemConfig, ZkStackConfig};

use super::{args::recovery::RecoveryArgs, utils::install_and_build_dependencies};
use crate::commands::dev::messages::{MSG_RECOVERY_TEST_RUN_INFO, MSG_RECOVERY_TEST_RUN_SUCCESS};

const RECOVERY_TESTS_PATH: &str = "core/tests/recovery-test";

pub async fn run(shell: &Shell, args: RecoveryArgs) -> anyhow::Result<()> {
    let config = ZkStackConfig::ecosystem(shell)?;

    logger::info(MSG_RECOVERY_TEST_RUN_INFO);
    Server::new(None, None, config.link_to_code().clone(), false).build(shell)?;

    if !args.no_deps {
        install_and_build_dependencies(shell, &config.link_to_code())?;
    }

    shell.change_dir(config.link_to_code().join(RECOVERY_TESTS_PATH));
    run_test(shell, &args, &config).await?;
    logger::outro(MSG_RECOVERY_TEST_RUN_SUCCESS);

    Ok(())
}

async fn run_test(
    shell: &Shell,
    args: &RecoveryArgs,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<()> {
    Spinner::new("Running test...").freeze();
    let cmd = if args.snapshot {
        cmd!(shell, "yarn mocha tests/snapshot-recovery.test.ts")
    } else {
        cmd!(shell, "yarn mocha tests/genesis-recovery.test.ts")
    };
    let cmd = Cmd::new(cmd)
        .env("CHAIN_NAME", ecosystem_config.current_chain())
        .env("NO_KILL", args.no_kill.to_string());

    cmd.with_force_run().run()?;

    Ok(())
}
