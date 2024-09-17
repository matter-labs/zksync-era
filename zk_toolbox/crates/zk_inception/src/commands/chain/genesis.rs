use anyhow::Context;
use common::{
    config::global_config,
    logger,
    server::{Server, ServerMode},
    spinner::Spinner,
};
use config::{
    override_config, set_databases, set_file_artifacts, set_rocks_db_config,
    traits::{FileConfigWithDefaultName, SaveConfigWithBasePath},
    ChainConfig, ContractsConfig, EcosystemConfig, FileArtifacts, GeneralConfig, GenesisConfig,
    SecretsConfig, WalletsConfig,
};
use types::ProverMode;
use xshell::Shell;
use zksync_basic_types::commitment::L1BatchCommitmentMode;

use crate::{
    commands::{
        chain::{
            args::genesis::{GenesisArgs, GenesisArgsFinal},
            database::initialize_databases,
        }
    },
    consts::{
        PATH_TO_ONLY_REAL_PROOFS_OVERRIDE_CONFIG, PATH_TO_VALIDIUM_OVERRIDE_CONFIG,
    },
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_FAILED_TO_RUN_SERVER_ERR,
        MSG_GENESIS_COMPLETED, MSG_RECREATE_ROCKS_DB_ERRROR, 
        MSG_SELECTED_CONFIG, MSG_STARTING_GENESIS,
        MSG_STARTING_GENESIS_SPINNER, MSG_INITIALIZING_DATABASES_SPINNER,
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
    let file_artifacts = FileArtifacts::new(config.artifacts.clone());
    set_rocks_db_config(&mut general, rocks_db)?;
    set_file_artifacts(&mut general, file_artifacts);
    general.save_with_base_path(shell, &config.configs)?;

    if config.prover_version != ProverMode::NoProofs {
        override_config(
            shell,
            config.link_to_code.join(PATH_TO_ONLY_REAL_PROOFS_OVERRIDE_CONFIG),
            config,
        )?;
    }

    if config.l1_batch_commit_data_generator_mode == L1BatchCommitmentMode::Validium {
        override_config(
            shell,
            config.link_to_code.join(PATH_TO_VALIDIUM_OVERRIDE_CONFIG),
            config,
        )?;
    }

    let mut secrets = config.get_secrets_config()?;
    set_databases(&mut secrets, &args.database_args.server_db, &args.database_args.prover_db)?;
    secrets.save_with_base_path(shell, &config.configs)?;

    logger::note(
        MSG_SELECTED_CONFIG,
        logger::object_to_string(serde_json::json!({
            "chain_config": config,
            "server_db_config": args.database_args.server_db,
            "prover_db_config": args.database_args.prover_db,
        })),
    );
    logger::info(MSG_STARTING_GENESIS);

    let spinner = Spinner::new(MSG_INITIALIZING_DATABASES_SPINNER);
    initialize_databases(
        shell,
        &args.database_args.server_db,
        &args.database_args.prover_db,
        args.database_args.link_to_code.clone(),
        args.database_args.dont_drop,
    )
    .await?;
    spinner.finish();

    let spinner = Spinner::new(MSG_STARTING_GENESIS_SPINNER);
    run_server_genesis(config, shell)?;
    spinner.finish();

    Ok(())
}

fn run_server_genesis(chain_config: &ChainConfig, shell: &Shell) -> anyhow::Result<()> {
    let server = Server::new(None, chain_config.link_to_code.clone(), false);
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
