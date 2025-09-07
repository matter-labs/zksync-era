use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, logger, spinner::Spinner};
use zkstack_cli_config::{ZkStackConfig, ZkStackConfigTrait};

use super::{args::upgrade::UpgradeArgs, utils::install_and_build_dependencies};
use crate::commands::dev::messages::{MSG_UPGRADE_TEST_RUN_INFO, MSG_UPGRADE_TEST_RUN_SUCCESS};

const UPGRADE_TESTS_PATH: &str = "core/tests/upgrade-test";

pub fn run(shell: &Shell, args: UpgradeArgs) -> anyhow::Result<()> {
    let config = ZkStackConfig::current_chain(shell)?;
    shell.change_dir(config.link_to_code().join(UPGRADE_TESTS_PATH));

    logger::info(MSG_UPGRADE_TEST_RUN_INFO);

    if !args.no_deps {
        install_and_build_dependencies(shell, &config.link_to_code())?;
    }

    run_test(shell, &config.name)?;
    logger::outro(MSG_UPGRADE_TEST_RUN_SUCCESS);

    Ok(())
}

fn run_test(shell: &Shell, chain_name: &str) -> anyhow::Result<()> {
    Spinner::new(MSG_UPGRADE_TEST_RUN_INFO).freeze();
    let cmd =
        Cmd::new(cmd!(shell, "yarn mocha tests/upgrade.test.ts")).env("CHAIN_NAME", chain_name);
    cmd.with_force_run().run()?;

    Ok(())
}
