use std::path::PathBuf;

use anyhow::Context;
use common::{
    config::global_config,
    db::{drop_db_if_exists, init_db, migrate_db, DatabaseConfig},
    logger,
};
use config::{
    override_config, set_file_artifacts, set_rocks_db_config, set_server_database,
    traits::SaveConfigWithBasePath, ChainConfig, EcosystemConfig, FileArtifacts,
};
use types::ProverMode;
use xshell::Shell;
use zksync_basic_types::commitment::L1BatchCommitmentMode;

use crate::{
    commands::chain::args::genesis::{GenesisArgs, GenesisArgsFinal},
    consts::{
        PATH_TO_ONLY_REAL_PROOFS_OVERRIDE_CONFIG, PATH_TO_VALIDIUM_OVERRIDE_CONFIG,
        SERVER_MIGRATIONS,
    },
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_FAILED_TO_DROP_SERVER_DATABASE_ERR,
        MSG_GENESIS_DATABASES_INITIALIZED, MSG_INITIALIZING_SERVER_DATABASE,
        MSG_RECREATE_ROCKS_DB_ERRROR,
    },
    utils::rocks_db::{recreate_rocksdb_dirs, RocksDBDirOption},
};

pub async fn run(args: GenesisArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    let mut secrets = chain_config.get_secrets_config()?;
    let args = args.fill_values_with_secrets(&chain_config)?;
    set_server_database(&mut secrets, &args.server_db)?;
    secrets.save_with_base_path(shell, &chain_config.configs)?;

    initialize_server_database(
        shell,
        &args.server_db,
        chain_config.link_to_code.clone(),
        args.dont_drop,
    )
    .await?;
    logger::outro(MSG_GENESIS_DATABASES_INITIALIZED);

    Ok(())
}

pub async fn initialize_server_database(
    shell: &Shell,
    server_db_config: &DatabaseConfig,
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

    Ok(())
}

pub fn update_configs(
    args: GenesisArgsFinal,
    shell: &Shell,
    config: &ChainConfig,
) -> anyhow::Result<()> {
    shell.create_dir(&config.rocks_db_path)?;

    // Update secrets configs
    let mut secrets = config.get_secrets_config()?;
    set_server_database(&mut secrets, &args.server_db)?;
    secrets.save_with_base_path(shell, &config.configs)?;

    // Update general config
    let mut general = config.get_general_config()?;
    let rocks_db = recreate_rocksdb_dirs(shell, &config.rocks_db_path, RocksDBDirOption::Main)
        .context(MSG_RECREATE_ROCKS_DB_ERRROR)?;
    let file_artifacts = FileArtifacts::new(config.artifacts.clone());
    set_rocks_db_config(&mut general, rocks_db)?;
    set_file_artifacts(&mut general, file_artifacts);
    general.save_with_base_path(shell, &config.configs)?;

    let link_to_code = config.link_to_code.clone();
    if config.prover_version != ProverMode::NoProofs {
        override_config(
            shell,
            link_to_code.join(PATH_TO_ONLY_REAL_PROOFS_OVERRIDE_CONFIG),
            config,
        )?;
    }
    if config.l1_batch_commit_data_generator_mode == L1BatchCommitmentMode::Validium {
        override_config(
            shell,
            link_to_code.join(PATH_TO_VALIDIUM_OVERRIDE_CONFIG),
            config,
        )?;
    }
    Ok(())
}
