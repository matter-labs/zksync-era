use common::{cmd::Cmd, logger, spinner::Spinner};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use crate::messages::{
    MSG_INTEGRATION_TESTS_BUILDING_CONTRACTS, MSG_INTEGRATION_TESTS_BUILDING_DEPENDENCIES,
    MSG_INTEGRATION_TESTS_RUN_INFO, MSG_INTEGRATION_TESTS_RUN_SUCCESS,
};

const TS_INTEGRATION_PATH: &str = "core/tests/ts-integration";
const CONTRACTS_TEST_DATA_PATH: &str = "etc/contracts-test-data";

pub fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    shell.change_dir(ecosystem_config.link_to_code.join(TS_INTEGRATION_PATH));

    logger::info(MSG_INTEGRATION_TESTS_RUN_INFO);

    build_repository(shell, &ecosystem_config)?;
    build_test_contracts(shell, &ecosystem_config)?;

    Cmd::new(
        cmd!(shell, "yarn jest --forceExit --testTimeout 60000")
            .env("CHAIN_NAME", ecosystem_config.default_chain),
    )
    .with_force_run()
    .run()?;

    logger::outro(MSG_INTEGRATION_TESTS_RUN_SUCCESS);

    Ok(())
}

fn build_repository(shell: &Shell, ecosystem_config: &EcosystemConfig) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(&ecosystem_config.link_to_code);
    let spinner = Spinner::new(MSG_INTEGRATION_TESTS_BUILDING_DEPENDENCIES);

    Cmd::new(cmd!(shell, "yarn install --frozen-lockfile")).run()?;
    Cmd::new(cmd!(shell, "yarn utils build")).run()?;

    spinner.finish();
    Ok(())
}

fn build_test_contracts(shell: &Shell, ecosystem_config: &EcosystemConfig) -> anyhow::Result<()> {
    let spinner = Spinner::new(MSG_INTEGRATION_TESTS_BUILDING_CONTRACTS);

    Cmd::new(cmd!(shell, "yarn build")).run()?;
    Cmd::new(cmd!(shell, "yarn build-yul")).run()?;

    let _dir_guard = shell.push_dir(ecosystem_config.link_to_code.join(CONTRACTS_TEST_DATA_PATH));
    Cmd::new(cmd!(shell, "yarn build")).run()?;

    spinner.finish();
    Ok(())
}
