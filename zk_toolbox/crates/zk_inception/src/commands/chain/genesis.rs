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
    server::{RunServer, ServerMode},
};

const SERVER_MIGRATIONS: &str = "core/lib/dal/migrations";
const PROVER_MIGRATIONS: &str = "prover/prover_dal/migrations";

pub async fn run(args: GenesisArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_name = global_config().chain_name.clone();
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context("Chain not initialized. Please create a chain first")?;
    let args = args.fill_values_with_prompt(&chain_config);

    genesis(args, shell, &chain_config).await?;
    logger::outro("Genesis completed successfully");

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
        "Selected config:",
        logger::object_to_string(serde_json::json!({
            "chain_config": config,
            "server_db_config": args.server_db,
            "prover_db_config": args.prover_db,
        })),
    );
    logger::info("Starting genesis process");

    let spinner = Spinner::new("Initializing databases...");
    initialize_databases(
        shell,
        &args.server_db,
        &args.prover_db,
        config.link_to_code.clone(),
        args.dont_drop,
    )
    .await?;
    spinner.finish();

    let spinner = Spinner::new(
        "Starting the genesis of the server. Building the entire server may take a lot of time...",
    );
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
        logger::debug("Initializing server database")
    }
    if !dont_drop {
        drop_db_if_exists(server_db_config)
            .await
            .context("Failed to drop server database")?;
        init_db(server_db_config).await?;
    }
    migrate_db(
        shell,
        path_to_server_migration,
        &server_db_config.full_url(),
    )
    .await?;

    if global_config().verbose {
        logger::debug("Initializing prover database")
    }
    if !dont_drop {
        drop_db_if_exists(prover_db_config)
            .await
            .context("Failed to drop prover database")?;
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
    server.run(shell, ServerMode::Genesis)
}
