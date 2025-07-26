use std::path::Path;

use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, logger, spinner::Spinner};
use zkstack_cli_config::ZkStackConfig;

use super::args::DatabaseCommonArgs;
use crate::commands::dev::{
    dals::{get_dals, Dal},
    messages::{
        msg_database_info, msg_database_loading, msg_database_success, MSG_DATABASE_MIGRATE_GERUND,
        MSG_DATABASE_MIGRATE_PAST, MSG_NO_DATABASES_SELECTED,
    },
};

pub async fn run(shell: &Shell, args: DatabaseCommonArgs) -> anyhow::Result<()> {
    let args = args.parse();
    if args.selected_dals.none() {
        logger::outro(MSG_NO_DATABASES_SELECTED);
        return Ok(());
    }

    logger::info(msg_database_info(MSG_DATABASE_MIGRATE_GERUND));
    let config = ZkStackConfig::from_file(shell)?;

    let dals = get_dals(shell, &args.selected_dals, &args.urls).await?;
    for dal in dals {
        migrate_database(shell, config.link_to_code(), dal)?;
    }

    logger::outro(msg_database_success(MSG_DATABASE_MIGRATE_PAST));

    Ok(())
}

fn migrate_database(shell: &Shell, link_to_code: impl AsRef<Path>, dal: Dal) -> anyhow::Result<()> {
    let dir = link_to_code.as_ref().join(&dal.path);
    let _dir_guard = shell.push_dir(dir);
    let url = dal.url.as_str();

    let spinner = Spinner::new(&msg_database_loading(
        MSG_DATABASE_MIGRATE_GERUND,
        &dal.path,
    ));
    Cmd::new(cmd!(shell, "cargo sqlx migrate run --database-url {url}")).run()?;
    spinner.finish();

    Ok(())
}
