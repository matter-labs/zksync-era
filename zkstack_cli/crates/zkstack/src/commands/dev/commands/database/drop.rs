use xshell::Shell;
use zkstack_cli_common::{
    db::{drop_db_if_exists, DatabaseConfig},
    logger,
    spinner::Spinner,
};

use super::args::DatabaseCommonArgs;
use crate::commands::dev::{
    dals::{get_dals, Dal},
    messages::{
        msg_database_info, msg_database_loading, msg_database_success, MSG_DATABASE_DROP_GERUND,
        MSG_DATABASE_DROP_PAST, MSG_NO_DATABASES_SELECTED,
    },
};

pub async fn run(shell: &Shell, args: DatabaseCommonArgs) -> anyhow::Result<()> {
    let args = args.parse();
    if args.selected_dals.none() {
        logger::outro(MSG_NO_DATABASES_SELECTED);
        return Ok(());
    }

    logger::info(msg_database_info(MSG_DATABASE_DROP_GERUND));

    let dals = get_dals(shell, &args.selected_dals, &args.urls).await?;
    for dal in dals {
        drop_database(dal).await?;
    }

    logger::outro(msg_database_success(MSG_DATABASE_DROP_PAST));

    Ok(())
}

pub async fn drop_database(dal: Dal) -> anyhow::Result<()> {
    let spinner = Spinner::new(&msg_database_loading(MSG_DATABASE_DROP_GERUND, &dal.path));
    let db = DatabaseConfig::from_url(&dal.url)?;
    drop_db_if_exists(&db).await?;
    spinner.finish();
    Ok(())
}
