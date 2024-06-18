use std::path::PathBuf;

use anyhow::Context;
use common::{
    config::global_config,
    db::{drop_db_if_exists, init_db, migrate_db, DatabaseConfig},
    logger,
    spinner::Spinner,
};
use config::{ChainConfig, EcosystemConfig};
use xshell::Shell;

use super::args::genesis::GenesisArgsFinal;
use crate::{
    commands::chain::args::genesis::GenesisArgs,
    config_manipulations::{update_database_secrets, update_general_config},
    consts::{PROVER_MIGRATIONS, SERVER_MIGRATIONS},
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_FAILED_TO_DROP_PROVER_DATABASE_ERR,
        MSG_FAILED_TO_DROP_SERVER_DATABASE_ERR, MSG_GENESIS_COMPLETED,
        MSG_INITIALIZING_DATABASES_SPINNER, MSG_INITIALIZING_PROVER_DATABASE,
        MSG_INITIALIZING_SERVER_DATABASE, MSG_SELECTED_CONFIG, MSG_STARTING_GENESIS,
        MSG_STARTING_GENESIS_SPINNER,
    },
    server::{RunServer, ServerMode},
};

pub async fn run(args: GenesisArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_name = global_config().chain_name.clone();
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let args = args.fill_values_with_prompt(&chain_config);

    genesis(args, shell, &chain_config).await?;
    logger::outro(MSG_GENESIS_COMPLETED);

    Ok(())
}

pub async fn genesis(
    args: GenesisArgsFinal,
    shell: &Shell,
    config: &ChainConfig,
) -> anyhow::Result<()> {
    // Clean the rocksdb
    shell.remove_path(&config.rocks_db_path)?;
    shell.create_dir(&config.rocks_db_path)?;

    update_general_config(shell, config)?;
    update_database_secrets(shell, config, &args.server_db, &args.prover_db)?;

    logger::note(
        MSG_SELECTED_CONFIG,
        logger::object_to_string(serde_json::json!({
            "chain_config": config,
            "server_db_config": args.server_db,
            "prover_db_config": args.prover_db,
        })),
    );
    logger::info(MSG_STARTING_GENESIS);

    let spinner = Spinner::new(MSG_INITIALIZING_DATABASES_SPINNER);
    initialize_databases(
        shell,
        &args.server_db,
        &args.prover_db,
        config.link_to_code.clone(),
        args.dont_drop,
    )
    .await?;
    spinner.finish();

    let spinner = Spinner::new(MSG_STARTING_GENESIS_SPINNER);
    run_server_genesis(config, shell)?;
    spinner.finish();

    Ok(())
}

async fn initialize_databases(
    shell: &Shell,
    server_db_config: &DatabaseConfig,
    prover_db_config: &DatabaseConfig,
    link_to_code: PathBuf,
    dont_drop: bool,
) -> anyhow::Result<()> {
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

fn run_server_genesis(chain_config: &ChainConfig, shell: &Shell) -> anyhow::Result<()> {
    let server = RunServer::new(None, chain_config);
    server.run(shell, ServerMode::Genesis, vec![])
}
