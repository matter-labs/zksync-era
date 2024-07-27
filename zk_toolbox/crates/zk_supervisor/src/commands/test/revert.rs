use common::{cmd::Cmd, logger, spinner::Spinner};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use super::args::revert::RevertArgs;
use crate::messages::{
    msg_revert_tests_run, MSG_REVERT_TEST_INSTALLING_DEPENDENCIES, MSG_REVERT_TEST_RUN_INFO,
    MSG_REVERT_TEST_RUN_SUCCESS,
};

const REVERT_TESTS_PATH: &str = "core/tests/revert-test";

pub fn run(shell: &Shell, args: RevertArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    shell.change_dir(ecosystem_config.link_to_code.join(REVERT_TESTS_PATH));

    logger::info(MSG_REVERT_TEST_RUN_INFO);
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
    let spinner = Spinner::new(MSG_REVERT_TEST_INSTALLING_DEPENDENCIES);
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
    Spinner::new(&msg_revert_tests_run(args.external_node)).freeze();

    let cmd = if args.external_node {
        cmd!(shell, "yarn mocha tests/revert-and-restart-en.test.ts")
    } else {
        cmd!(shell, "yarn mocha tests/revert-and-restart.test.ts")
    };

    let mut cmd = Cmd::new(cmd).env("CHAIN_NAME", &ecosystem_config.default_chain);
    if args.enable_consensus {
        cmd = cmd.env("ENABLE_CONSENSUS", "true");
    }
    cmd.with_force_run().run()?;

    Ok(())
}
