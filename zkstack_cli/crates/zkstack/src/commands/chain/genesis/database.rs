use std::path::Path;

use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config,
    db::{drop_db_if_exists, init_db, migrate_db, DatabaseConfig},
    logger,
};
use zkstack_cli_config::{override_config, ChainConfig, FileArtifacts, ZkStackConfig};
use zkstack_cli_types::ProverMode;
use zksync_basic_types::commitment::L1BatchCommitmentMode;

use crate::{
    commands::chain::args::genesis::{GenesisArgs, GenesisArgsFinal},
    consts::{
        PATH_TO_ONLY_REAL_PROOFS_OVERRIDE_CONFIG, PATH_TO_VALIDIUM_OVERRIDE_CONFIG,
        SERVER_MIGRATIONS,
    },
    messages::{
        MSG_FAILED_TO_DROP_SERVER_DATABASE_ERR, MSG_GENESIS_DATABASES_INITIALIZED,
        MSG_INITIALIZING_SERVER_DATABASE, MSG_RECREATE_ROCKS_DB_ERRROR,
    },
    utils::rocks_db::{recreate_rocksdb_dirs, RocksDBDirOption},
};

pub async fn run(args: GenesisArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_config = ZkStackConfig::current_chain(shell)?;

    let mut secrets = chain_config.get_secrets_config().await?.patched();
    let args = args.fill_values_with_secrets(&chain_config).await?;
    secrets.set_server_database(&args.server_db)?;
    secrets.save().await?;

    initialize_server_database(
        shell,
        &args.server_db,
        &chain_config.link_to_code,
        args.dont_drop,
    )
    .await?;
    logger::outro(MSG_GENESIS_DATABASES_INITIALIZED);

    Ok(())
}

pub async fn initialize_server_database(
    shell: &Shell,
    server_db_config: &DatabaseConfig,
    link_to_code: &Path,
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

pub async fn update_configs(
    args: GenesisArgsFinal,
    shell: &Shell,
    config: &ChainConfig,
    override_validium_config: bool,
) -> anyhow::Result<()> {
    shell.create_dir(&config.rocks_db_path)?;

    // Update secrets configs
    let mut secrets = config.get_secrets_config().await?.patched();
    secrets.set_server_database(&args.server_db)?;
    secrets.save().await?;

    // Update general config
    let mut general = config.get_general_config().await?.patched();
    let rocks_db = recreate_rocksdb_dirs(shell, &config.rocks_db_path, RocksDBDirOption::Main)
        .context(MSG_RECREATE_ROCKS_DB_ERRROR)?;
    let file_artifacts = FileArtifacts::new(config.artifacts.clone());
    general.set_rocks_db_config(rocks_db)?;
    general.set_file_artifacts(file_artifacts)?;
    general.save().await?;

    let link_to_code = config.link_to_code.clone();
    if config.prover_version != ProverMode::NoProofs {
        override_config(
            shell,
            link_to_code.join(PATH_TO_ONLY_REAL_PROOFS_OVERRIDE_CONFIG),
            config,
        )?;
    }
    if override_validium_config
        && config.l1_batch_commit_data_generator_mode == L1BatchCommitmentMode::Validium
    {
        override_config(
            shell,
            link_to_code.join(PATH_TO_VALIDIUM_OVERRIDE_CONFIG),
            config,
        )?;
    }
    Ok(())
}
