use xshell::Shell;
use zkstack_cli_common::{cmd::Cmd, logger};

use super::{args::fees::FeesArgs, integration::IntegrationTestRunner};
use crate::commands::dev::messages::MSG_INTEGRATION_TESTS_RUN_SUCCESS;

pub async fn run(shell: &Shell, args: FeesArgs) -> anyhow::Result<()> {
    let runner = IntegrationTestRunner::new(shell, args.no_deps)?;

    logger::info(format!(
        "Running fees tests on chain: {}",
        runner.current_chain()
    ));

    let command = runner
        .with_test_suite("fees")
        .build_command()
        .await?
        .env("SPAWN_NODE", "1")
        .env("RUN_FEE_TEST", "1")
        .env("NO_KILL", args.no_kill.to_string());
    Cmd::new(command).with_force_run().run()?;
    logger::outro(MSG_INTEGRATION_TESTS_RUN_SUCCESS);
    Ok(())
}
