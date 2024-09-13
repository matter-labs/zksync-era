use std::{path::Path, str::FromStr};

use anyhow::Context;
use common::{cmd::Cmd, db::wait_for_db, logger};
use config::EcosystemConfig;
use url::Url;
use xshell::{cmd, Shell};

use super::args::rust::RustArgs;
use crate::{
    commands::database,
    dals::{Dal, CORE_DAL_PATH, PROVER_DAL_PATH},
    defaults::{TEST_DATABASE_PROVER_URL, TEST_DATABASE_SERVER_URL},
    messages::{
        MSG_CHAIN_NOT_FOUND_ERR, MSG_POSTGRES_CONFIG_NOT_FOUND_ERR, MSG_RESETTING_TEST_DATABASES,
        MSG_UNIT_TESTS_RUN_SUCCESS, MSG_USING_CARGO_NEXTEST,
    },
};

pub async fn run(shell: &Shell, args: RustArgs) -> anyhow::Result<()> {
    let ecosystem = EcosystemConfig::from_file(shell)?;
    let chain = ecosystem
        .clone()
        .load_chain(Some(ecosystem.default_chain))
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    let general_config = chain.get_general_config();
    let link_to_code = ecosystem.link_to_code;

    let (test_server_url, test_prover_url) = if let Ok(general_config) = general_config {
        let postgres = general_config
            .postgres_config
            .context(MSG_POSTGRES_CONFIG_NOT_FOUND_ERR)?;

        (
            postgres
                .test_server_url
                .context(MSG_POSTGRES_CONFIG_NOT_FOUND_ERR)?,
            postgres
                .test_prover_url
                .context(MSG_POSTGRES_CONFIG_NOT_FOUND_ERR)?,
        )
    } else {
        (
            TEST_DATABASE_SERVER_URL.to_string(),
            TEST_DATABASE_PROVER_URL.to_string(),
        )
    };

    let dals = vec![
        Dal {
            url: Url::from_str(&test_server_url.clone())?,
            path: CORE_DAL_PATH.to_string(),
        },
        Dal {
            url: Url::from_str(&test_prover_url.clone())?,
            path: PROVER_DAL_PATH.to_string(),
        },
    ];

    reset_test_databases(shell, &link_to_code, dals).await?;

    let _dir_guard = shell.push_dir(&link_to_code);

    logger::info(MSG_USING_CARGO_NEXTEST);
    let cmd = cmd!(shell, "cargo nextest run --release");

    let cmd = if let Some(options) = args.options {
        Cmd::new(cmd.args(options.split_whitespace())).with_force_run()
    } else {
        Cmd::new(cmd).with_force_run()
    };

    let cmd = cmd
        .env("TEST_DATABASE_URL", test_server_url)
        .env("TEST_PROVER_DATABASE_URL", test_prover_url);
    cmd.run()?;

    logger::outro(MSG_UNIT_TESTS_RUN_SUCCESS);
    Ok(())
}

async fn reset_test_databases(
    shell: &Shell,
    link_to_code: &Path,
    dals: Vec<Dal>,
) -> anyhow::Result<()> {
    logger::info(MSG_RESETTING_TEST_DATABASES);

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

    for dal in dals {
        let mut url = dal.url.clone();
        url.set_path("");
        wait_for_db(&url, 3).await?;
        database::reset::reset_database(shell, link_to_code, dal.clone()).await?;
    }

    Ok(())
}
