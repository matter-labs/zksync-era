use common::{cmd::Cmd, logger, server::Server, spinner::Spinner};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use super::args::revert_and_restart::RevertAndRestartArgs;
use crate::messages::{
    MSG_TEST_REVERT_AND_RESTART_RUN_INFO, MSG_TEST_REVERT_AND_RESTART_RUN_SUCCESS,
};

const REVERT_TESTS_PATH: &str = "core/tests/revert-test";

pub fn run(shell: &Shell, args: RevertAndRestartArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    shell.change_dir(ecosystem_config.link_to_code.join(REVERT_TESTS_PATH));

    logger::info(MSG_TEST_REVERT_AND_RESTART_RUN_INFO);
    Server::new(None, ecosystem_config.link_to_code.clone()).build(shell)?;
    run_test(shell, &args, &ecosystem_config)?;
    logger::outro(MSG_TEST_REVERT_AND_RESTART_RUN_SUCCESS);

    Ok(())
}

fn run_test(
    shell: &Shell,
    args: &RevertAndRestartArgs,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<()> {
    Spinner::new("Running test...").freeze();

    let mut cmd = Cmd::new(cmd!(shell, "yarn mocha tests/revert-and-restart.test.ts"))
        .env("CHAIN_NAME", &ecosystem_config.default_chain);
    if args.enable_consensus {
        cmd = cmd.env("ENABLE_CONSENSUS", "true");
    }
    cmd.with_force_run().run()?;

    Ok(())
}
