use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, logger, spinner::Spinner};
use zkstack_cli_config::EcosystemConfig;

use super::utils::install_and_build_dependencies;
use crate::commands::dev::{
    commands::test::args::gateway_migration::GatewayMigrationArgs,
    messages::{MSG_GATEWAY_UPGRADE_TEST_RUN_INFO, MSG_GATEWAY_UPGRADE_TEST_RUN_SUCCESS},
};

const GATEWAY_SWITCH_TESTS_PATH: &str = "core/tests/gateway-migration-test";

pub fn run(shell: &Shell, args: GatewayMigrationArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    shell.change_dir(
        ecosystem_config
            .link_to_code
            .join(GATEWAY_SWITCH_TESTS_PATH),
    );

    logger::info(MSG_GATEWAY_UPGRADE_TEST_RUN_INFO);

    if !args.no_deps {
        install_and_build_dependencies(shell, &ecosystem_config)?;
    }

    run_test(
        shell,
        &ecosystem_config,
        args.direction.to_gateway,
        args.gateway_chain,
    )?;
    logger::outro(MSG_GATEWAY_UPGRADE_TEST_RUN_SUCCESS);

    Ok(())
}

fn run_test(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    to_gateway: bool,
    gateway_chain: Option<String>,
) -> anyhow::Result<()> {
    Spinner::new(MSG_GATEWAY_UPGRADE_TEST_RUN_INFO).freeze();
    let direction = if to_gateway { "TO" } else { "FROM" };
    let mut cmd = Cmd::new(cmd!(shell, "yarn mocha tests/migration.test.ts"))
        .env("CHAIN_NAME", ecosystem_config.current_chain())
        .env("DIRECTION", direction);
    if let Some(chain) = gateway_chain {
        cmd = cmd.env("GATEWAY_CHAIN", chain);
    }
    cmd.with_force_run().run()?;

    Ok(())
}
