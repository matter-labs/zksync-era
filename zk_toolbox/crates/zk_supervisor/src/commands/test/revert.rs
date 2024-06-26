use common::{cmd::Cmd, logger, server::Server, spinner::Spinner};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use super::args::revert_and_restart::RevertArgs;
use crate::messages::{MSG_REVERT_TEST_RUN_INFO, MSG_REVERT_TEST_RUN_SUCCESS};

const REVERT_TESTS_PATH: &str = "core/tests/revert-test";

pub fn run(shell: &Shell, args: RevertArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    shell.change_dir(ecosystem_config.link_to_code.join(REVERT_TESTS_PATH));

    logger::info(MSG_REVERT_TEST_RUN_INFO);
    Server::new(None, ecosystem_config.link_to_code.clone()).build(shell)?;
    install_and_build_dependencies(shell, &ecosystem_config)?;
    run_test(shell, &args, &ecosystem_config)?;
    logger::outro(MSG_REVERT_TEST_RUN_SUCCESS);

    Ok(())
}

fn install_and_build_dependencies(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(&ecosystem_config.link_to_code);
    let spinner = Spinner::new("Installing and building dependencies...");
    Cmd::new(cmd!(shell, "yarn install")).run()?;
    Cmd::new(cmd!(shell, "yarn utils build")).run()?;
    spinner.finish();
    Ok(())
}

fn run_test(
    shell: &Shell,
    args: &RevertArgs,
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
