use std::path::Path;

use common::logger;
use config::EcosystemConfig;
use xshell::Shell;

use super::{args::DatabaseCommonArgs, drop::drop_database, setup::setup_database};
use crate::{
    dals::{get_dals, Dal},
    messages::{
        msg_database_info, msg_database_loading, msg_database_success, MSG_DATABASE_RESET_GERUND,
        MSG_DATABASE_RESET_PAST, MSG_NO_DATABASES_SELECTED,
    },
};

pub async fn run(shell: &Shell, args: DatabaseCommonArgs) -> anyhow::Result<()> {
    let args = args.parse();
    if args.selected_dals.none() {
        logger::outro(MSG_NO_DATABASES_SELECTED);
        return Ok(());
    }

    let ecoseystem_config = EcosystemConfig::from_file(shell)?;

    logger::info(msg_database_info(MSG_DATABASE_RESET_GERUND));

    let dals = get_dals(shell, &args.selected_dals)?;
    for dal in dals {
        logger::info(&msg_database_loading(MSG_DATABASE_RESET_GERUND, &dal.path));
        reset_database(shell, ecoseystem_config.link_to_code.clone(), dal).await?;
    }

    logger::outro(msg_database_success(MSG_DATABASE_RESET_PAST));

    Ok(())
}

pub async fn reset_database(
    shell: &Shell,
    link_to_code: impl AsRef<Path>,
    dal: Dal,
) -> anyhow::Result<()> {
    drop_database(dal.clone()).await?;
    setup_database(shell, link_to_code, dal)?;
    Ok(())
}
