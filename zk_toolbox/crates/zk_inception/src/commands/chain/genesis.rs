use std::path::PathBuf;

use anyhow::Context;
use common::{
    config::global_config,
    db::{drop_db_if_exists, init_db, migrate_db, DatabaseConfig},
    logger,
    server::{Server, ServerMode},
    spinner::Spinner,
};
use config::{
    set_databases, set_rocks_db_config,
    traits::{FileConfigWithDefaultName, SaveConfigWithBasePath},
    ChainConfig, ContractsConfig, EcosystemConfig, GeneralConfig, GenesisConfig, SecretsConfig,
    WalletsConfig,
};
use types::ProverMode;
use xshell::Shell;
use zksync_basic_types::commitment::L1BatchCommitmentMode;
use zksync_config::configs::eth_sender::{ProofSendingMode, PubdataSendingMode};

use super::args::genesis::GenesisArgsFinal;
use crate::{
    commands::chain::args::genesis::GenesisArgs,
    consts::{PROVER_MIGRATIONS, SERVER_MIGRATIONS},
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_FAILED_TO_DROP_PROVER_DATABASE_ERR,
        MSG_FAILED_TO_DROP_SERVER_DATABASE_ERR, MSG_FAILED_TO_RUN_SERVER_ERR,
        MSG_GENESIS_COMPLETED, MSG_INITIALIZING_DATABASES_SPINNER,
        MSG_INITIALIZING_PROVER_DATABASE, MSG_INITIALIZING_SERVER_DATABASE,
        MSG_RECREATE_ROCKS_DB_ERRROR, MSG_SELECTED_CONFIG, MSG_STARTING_GENESIS,
        MSG_STARTING_GENESIS_SPINNER,
    },
    utils::rocks_db::{recreate_rocksdb_dirs, RocksDBDirOption},
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
    shell.create_dir(&config.rocks_db_path)?;

    let rocks_db = recreate_rocksdb_dirs(shell, &config.rocks_db_path, RocksDBDirOption::Main)
        .context(MSG_RECREATE_ROCKS_DB_ERRROR)?;
    let mut general = config.get_general_config()?;
    set_rocks_db_config(&mut general, rocks_db)?;
    if config.prover_version != ProverMode::NoProofs {
        general
            .eth
            .as_mut()
            .context("eth")?
            .sender
            .as_mut()
            .context("sender")?
            .proof_sending_mode = ProofSendingMode::OnlyRealProofs;
    }

    if config.l1_batch_commit_data_generator_mode == L1BatchCommitmentMode::Validium {
        general
            .eth
            .as_mut()
            .context("eth")?
            .sender
            .as_mut()
            .context("sender")?
            .pubdata_sending_mode = PubdataSendingMode::Custom
    }

    general.save_with_base_path(shell, &config.configs)?;

    let mut secrets = config.get_secrets_config()?;
    set_databases(&mut secrets, &args.server_db, &args.prover_db)?;
    secrets.save_with_base_path(shell, &config.configs)?;

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
    let server = Server::new(None, chain_config.link_to_code.clone());
    server
        .run(
            shell,
            ServerMode::Genesis,
            GenesisConfig::get_path_with_base_path(&chain_config.configs),
            WalletsConfig::get_path_with_base_path(&chain_config.configs),
            GeneralConfig::get_path_with_base_path(&chain_config.configs),
            SecretsConfig::get_path_with_base_path(&chain_config.configs),
            ContractsConfig::get_path_with_base_path(&chain_config.configs),
            vec![],
        )
        .context(MSG_FAILED_TO_RUN_SERVER_ERR)
}
