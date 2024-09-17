use std::path::PathBuf;

use anyhow::Context;
use common::{
    config::global_config,
    db::{drop_db_if_exists, init_db, migrate_db, DatabaseConfig},
    logger,
    spinner::Spinner,
};
use xshell::Shell;

use crate::{
    consts::{PROVER_MIGRATIONS, SERVER_MIGRATIONS},
    messages::{
        MSG_FAILED_TO_DROP_PROVER_DATABASE_ERR,
        MSG_FAILED_TO_DROP_SERVER_DATABASE_ERR, 
        MSG_INITIALIZING_DATABASES_SPINNER,
        MSG_INITIALIZING_PROVER_DATABASE, MSG_INITIALIZING_SERVER_DATABASE,
    },
    commands::chain::args::database::{DatabaseArgs,DatabaseArgsFinal},
};

pub async fn run(shell: &Shell, args: DatabaseArgs) -> anyhow::Result<()> {
    let args = args.standalone_fill_values_with_prompt();
    database(args, shell).await?;
    Ok(())
}

pub async fn database(
    args: DatabaseArgsFinal,
    shell: &Shell,
) -> anyhow::Result<()> {
    let spinner = Spinner::new(MSG_INITIALIZING_DATABASES_SPINNER);
    initialize_databases(
        shell,
        &args.server_db,
        &args.prover_db,
        args.link_to_code,
        args.dont_drop,
    )
    .await?;
    spinner.finish();
    Ok(())
}

pub async fn initialize_databases(
    shell: &Shell,
    server_db_config: &DatabaseConfig,
    prover_db_config: &DatabaseConfig,
    path_to_code: PathBuf,
    dont_drop: bool,
) -> anyhow::Result<()> {
    //let link_to_code = PathBuf::from("/root/zksync-era");
    let path_to_server_migration = path_to_code.join(SERVER_MIGRATIONS);

    if global_config().verbose {
        logger::debug(MSG_INITIALIZING_SERVER_DATABASE)
    }
    if !dont_drop {
        drop_db_if_exists(server_db_config)
            .await
            .context(MSG_FAILED_TO_DROP_SERVER_DATABASE_ERR)?;
    }
    init_db(server_db_config).await?;
    migrate_db(
        shell,
        path_to_server_migration,
        &server_db_config.full_url(),
    )
    .await?;

    if global_config().verbose {
        logger::debug(MSG_INITIALIZING_PROVER_DATABASE)
    }
    if !dont_drop {
        drop_db_if_exists(prover_db_config)
            .await
            .context(MSG_FAILED_TO_DROP_PROVER_DATABASE_ERR)?;
    }
    init_db(prover_db_config).await?;
    let path_to_prover_migration = path_to_code.join(PROVER_MIGRATIONS);
    migrate_db(
        shell,
        path_to_prover_migration,
        &prover_db_config.full_url(),
    )
    .await?;

    Ok(())
}