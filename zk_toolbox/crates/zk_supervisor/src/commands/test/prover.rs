use std::str::FromStr;

use common::{cmd::Cmd, logger};
use config::EcosystemConfig;
use url::Url;
use xshell::{cmd, Shell};

use crate::{
    commands::test::db::reset_test_databases,
    dals::{Dal, PROVER_DAL_PATH},
    defaults::TEST_DATABASE_PROVER_URL,
    messages::MSG_PROVER_TEST_SUCCESS,
};

pub async fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem = EcosystemConfig::from_file(shell)?;
    let _dir_guard = shell.push_dir(ecosystem.link_to_code.join("prover"));
    let dals = vec![Dal {
        url: Url::from_str(&TEST_DATABASE_PROVER_URL)?,
        path: PROVER_DAL_PATH.to_string(),
    }];

    reset_test_databases(shell, &ecosystem.link_to_code, dals).await?;

    Cmd::new(cmd!(shell, "cargo test --release --workspace --locked"))
        .with_force_run()
        .env("TEST_DATABASE_PROVER_URL", TEST_DATABASE_PROVER_URL)
        .run()?;

    logger::outro(MSG_PROVER_TEST_SUCCESS);
    Ok(())
}
