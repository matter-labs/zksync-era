use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, logger, spinner::Spinner};
use zkstack_cli_config::{ChainConfig, ZkStackConfig, ZkStackConfigTrait};

use super::utils::install_and_build_dependencies;
use crate::commands::dev::{
    commands::test::args::token_balance_migration::TokenBalanceMigrationArgs,
    messages::{
        MSG_TOKEN_BALANCE_MIGRATION_TEST_RUN_INFO, MSG_TOKEN_BALANCE_MIGRATION_TEST_RUN_SUCCESS,
    },
};

const TOKEN_BALANCE_MIGRATION_TEST_PATH: &str = "core/tests/token-balance-migration-test";

pub fn run(shell: &Shell, args: TokenBalanceMigrationArgs) -> anyhow::Result<()> {
    let chain_config = ZkStackConfig::current_chain(shell)?;

    logger::info(MSG_TOKEN_BALANCE_MIGRATION_TEST_RUN_INFO);

    if !args.no_deps {
        install_and_build_dependencies(shell, &chain_config.link_to_code())?;
    }

    shell.change_dir(
        chain_config
            .link_to_code()
            .join(TOKEN_BALANCE_MIGRATION_TEST_PATH),
    );
    run_test(shell, &chain_config, args.gateway_chain)?;
    logger::outro(MSG_TOKEN_BALANCE_MIGRATION_TEST_RUN_SUCCESS);

    Ok(())
}

fn run_test(
    shell: &Shell,
    chain_config: &ChainConfig,
    gateway_chain: Option<String>,
) -> anyhow::Result<()> {
    Spinner::new(MSG_TOKEN_BALANCE_MIGRATION_TEST_RUN_INFO).freeze();
    let mut cmd = Cmd::new(cmd!(shell, "yarn mocha tests/migration.test.ts"))
        .env("CHAIN_NAME", &chain_config.name);
    if let Some(chain) = gateway_chain {
        cmd = cmd.env("GATEWAY_CHAIN", chain);
    }
    cmd.with_force_run().run()?;

    Ok(())
}
