use anyhow::Context;
use common::{cmd::Cmd, db::wait_for_db, logger};
use config::EcosystemConfig;
use xshell::{cmd, Shell};

use super::args::rust::RustArgs;
use crate::{
    commands::database,
    dals::get_test_dals,
    messages::{
        MSG_CARGO_NEXTEST_MISSING_ERR, MSG_CHAIN_NOT_FOUND_ERR, MSG_POSTGRES_CONFIG_NOT_FOUND_ERR,
        MSG_RESETTING_TEST_DATABASES, MSG_UNIT_TESTS_RUN_SUCCESS, MSG_USING_CARGO_NEXTEST,
    },
};

pub async fn run(shell: &Shell, args: RustArgs) -> anyhow::Result<()> {
    let ecosystem = EcosystemConfig::from_file(shell)?;
    let chain = ecosystem
        .clone()
        .load_chain(Some(ecosystem.default_chain))
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    let general_config = chain.get_general_config()?;
    let postgres = general_config
        .postgres_config
        .context(MSG_POSTGRES_CONFIG_NOT_FOUND_ERR)?;

    reset_test_databases(shell).await?;

    let _dir_guard = shell.push_dir(&ecosystem.link_to_code);

    let cmd = if nextest_is_installed(shell)? {
        logger::info(MSG_USING_CARGO_NEXTEST);
        cmd!(shell, "cargo nextest run --release")
    } else {
        logger::error(MSG_CARGO_NEXTEST_MISSING_ERR);
        cmd!(shell, "cargo test --release")
    };

    let cmd = if let Some(options) = args.options {
        Cmd::new(cmd.args(options.split_whitespace())).with_force_run()
    } else {
        Cmd::new(cmd).with_force_run()
    };

    let cmd = cmd
        .env(
            "TEST_DATABASE_URL",
            postgres
                .test_server_url
                .context(MSG_POSTGRES_CONFIG_NOT_FOUND_ERR)?,
        )
        .env(
            "TEST_PROVER_DATABASE_URL",
            postgres
                .test_prover_url
                .context(MSG_POSTGRES_CONFIG_NOT_FOUND_ERR)?,
        );
    cmd.run()?;

    logger::outro(MSG_UNIT_TESTS_RUN_SUCCESS);
    Ok(())
}

fn nextest_is_installed(shell: &Shell) -> anyhow::Result<bool> {
    let out = String::from_utf8(
        Cmd::new(cmd!(shell, "cargo install --list"))
            .run_with_output()?
            .stdout,
    )?;
    Ok(out.contains("cargo-nextest"))
}

async fn reset_test_databases(shell: &Shell) -> anyhow::Result<()> {
    logger::info(MSG_RESETTING_TEST_DATABASES);
    let ecosystem = EcosystemConfig::from_file(shell)?;

    Cmd::new(cmd!(
        shell,
        "docker compose -f docker-compose-unit-tests.yml down"
    ))
    .run()?;
    Cmd::new(cmd!(
        shell,
        "docker compose -f docker-compose-unit-tests.yml up -d"
    ))
    .run()?;

    for dal in get_test_dals(shell)? {
        wait_for_db(shell, &dal.url.clone(), 3).await?;
        database::reset::reset_database(shell, ecosystem.link_to_code.clone(), dal.clone()).await?;
    }

    Ok(())
}
