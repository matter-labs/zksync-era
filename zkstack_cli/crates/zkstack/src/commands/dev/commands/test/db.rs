use std::path::Path;

use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, db::wait_for_db, logger};

use crate::commands::dev::{commands::database, dals::Dal, messages::MSG_RESETTING_TEST_DATABASES};

pub async fn reset_test_databases(
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
        wait_for_db(&url, 20).await?;
        database::reset::reset_database(shell, link_to_code, dal.clone()).await?;
    }

    Ok(())
}
