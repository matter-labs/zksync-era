use common::{cmd::Cmd, config::global_config, logger};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use super::{
    args::fees::FeesArgs,
    utils::{build_contracts, install_and_build_dependencies, TS_INTEGRATION_PATH},
};
use crate::messages::{msg_integration_tests_run, MSG_INTEGRATION_TESTS_RUN_SUCCESS};

pub async fn run(shell: &Shell, args: FeesArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    shell.change_dir(ecosystem_config.link_to_code.join(TS_INTEGRATION_PATH));

    if !args.no_deps {
        logger::info("Installing dependencies");
        build_contracts(shell, &ecosystem_config)?;
        install_and_build_dependencies(shell, &ecosystem_config)?;
    }

    logger::info(format!(
        "Running fees tests on chain: {}",
        ecosystem_config.current_chain()
    ));

    let mut command = cmd!(shell, "yarn jest -- fees.test.ts")
        .env("CHAIN_NAME", ecosystem_config.current_chain());

    if global_config().verbose {
        command = command.env(
            "ZKSYNC_DEBUG_LOGS",
            format!("{:?}", global_config().verbose),
        )
    }

    Cmd::new(command).with_force_run().run()?;

    logger::outro(MSG_INTEGRATION_TESTS_RUN_SUCCESS);

    Ok(())
}
