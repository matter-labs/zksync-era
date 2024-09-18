use common::{cmd::Cmd, logger, spinner::Spinner};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use super::{args::upgrade::UpgradeArgs, utils::install_and_build_dependencies};
use crate::messages::{MSG_UPGRADE_TEST_RUN_INFO, MSG_UPGRADE_TEST_RUN_SUCCESS};

const UPGRADE_TESTS_PATH: &str = "core/tests/upgrade-test";

pub fn run(shell: &Shell, args: UpgradeArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    shell.change_dir(ecosystem_config.link_to_code.join(UPGRADE_TESTS_PATH));

    logger::info(MSG_UPGRADE_TEST_RUN_INFO);

    if !args.no_deps {
        install_and_build_dependencies(shell, &ecosystem_config)?;
    }

    run_test(shell, &ecosystem_config)?;
    logger::outro(MSG_UPGRADE_TEST_RUN_SUCCESS);

    Ok(())
}

fn run_test(shell: &Shell, ecosystem_config: &EcosystemConfig) -> anyhow::Result<()> {
    Spinner::new(MSG_UPGRADE_TEST_RUN_INFO).freeze();
    let cmd = Cmd::new(cmd!(shell, "yarn mocha tests/upgrade.test.ts"))
        .env("CHAIN_NAME", ecosystem_config.current_chain());
    cmd.with_force_run().run()?;

    Ok(())
}
