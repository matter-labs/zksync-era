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
};

use clap::Parser;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    messages::{
        MSG_PROVER_DB_NAME_HELP, MSG_PROVER_DB_URL_HELP,
        MSG_SERVER_DB_NAME_HELP, MSG_SERVER_DB_URL_HELP, 
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, Parser, Default)]
pub struct DatabaseArgs {
    #[clap(long, help = MSG_SERVER_DB_URL_HELP)]
    pub server_db_url: Option<Url>,
    #[clap(long, help = MSG_SERVER_DB_NAME_HELP)]
    pub server_db_name: Option<String>,
    #[clap(long, help = MSG_PROVER_DB_URL_HELP)]
    pub prover_db_url: Option<Url>,
    #[clap(long, help = MSG_PROVER_DB_NAME_HELP)]
    pub prover_db_name: Option<String>,
    #[clap(long, short, action)]
    pub dont_drop: bool,
}

impl DatabaseArgs {
    pub fn fill_values_with_prompt(self) -> DatabaseArgsFinal {
        // This is way unsafe and can just panic, but idc. Pass the right args.
        DatabaseArgsFinal {
            server_db: DatabaseConfig::new(self.server_db_url.unwrap(), self.server_db_name.unwrap()),
            prover_db: DatabaseConfig::new(self.prover_db_url.unwrap(), self.prover_db_name.unwrap()),
            dont_drop: self.dont_drop,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseArgsFinal {
    pub server_db: DatabaseConfig,
    pub prover_db: DatabaseConfig,
    pub dont_drop: bool,
}

pub async fn run(shell: &Shell, args: DatabaseArgs) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt();
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
        args.dont_drop,
    )
    .await?;
    spinner.finish();
    Ok(())
}

async fn initialize_databases(
    shell: &Shell,
    server_db_config: &DatabaseConfig,
    prover_db_config: &DatabaseConfig,
    dont_drop: bool,
) -> anyhow::Result<()> {
    // Opinions :P
    let link_to_code = PathBuf::from("/root/zksync-era");
    let path_to_server_migration = link_to_code.join(SERVER_MIGRATIONS);

    if global_config().verbose {
        logger::debug(MSG_INITIALIZING_SERVER_DATABASE)
    }
    if !dont_drop {
        drop_db_if_exists(server_db_config)
            .await
            .context(MSG_FAILED_TO_DROP_SERVER_DATABASE_ERR)?;
        init_db(server_db_config).await?;
    }
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
        init_db(prover_db_config).await?;
    }
    let path_to_prover_migration = link_to_code.join(PROVER_MIGRATIONS);
    migrate_db(
        shell,
        path_to_prover_migration,
        &prover_db_config.full_url(),
    )
    .await?;

    Ok(())
}