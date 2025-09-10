use std::str::FromStr;

use url::Url;
use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, logger};
use zkstack_cli_config::{get_link_to_prover, ZkStackConfig, ZkStackConfigTrait};

use crate::commands::dev::{
    commands::test::db::reset_test_databases,
    dals::{Dal, PROVER_DAL_PATH},
    defaults::TEST_DATABASE_PROVER_URL,
    messages::MSG_PROVER_TEST_SUCCESS,
};

pub async fn run(shell: &Shell) -> anyhow::Result<()> {
    let config = ZkStackConfig::from_file(shell)?;
    let dals = vec![Dal {
        url: Url::from_str(TEST_DATABASE_PROVER_URL)?,
        path: PROVER_DAL_PATH.to_string(),
    }];
    reset_test_databases(shell, &config.link_to_code(), dals).await?;

    let _dir_guard = shell.push_dir(get_link_to_prover(&config.link_to_code()));
    Cmd::new(cmd!(shell, "cargo test --release --workspace --locked"))
        .with_force_run()
        .env("TEST_DATABASE_PROVER_URL", TEST_DATABASE_PROVER_URL)
        .run()?;

    logger::outro(MSG_PROVER_TEST_SUCCESS);
    Ok(())
}
