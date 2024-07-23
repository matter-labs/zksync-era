use anyhow::Context;
use common::{
    config::global_config,
    db::{drop_db_if_exists, init_db, migrate_db, DatabaseConfig},
    spinner::Spinner,
};
use config::{traits::ReadConfigWithBasePath, ChainConfig, EcosystemConfig, SecretsConfig};
use xshell::Shell;

use crate::{
    consts::SERVER_MIGRATIONS,
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_EXTERNAL_NODE_CONFIG_NOT_INITIALIZED,
        MSG_FAILED_TO_DROP_SERVER_DATABASE_ERR, MSG_INITIALIZING_DATABASES_SPINNER,
    },
    utils::rocks_db::{recreate_rocksdb_dirs, RocksDBDirOption},
};

pub async fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let chain = global_config().chain_name.clone();
    let chain_config = ecosystem_config
        .load_chain(chain)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    init(shell, &chain_config).await
}

pub async fn init(shell: &Shell, chain_config: &ChainConfig) -> anyhow::Result<()> {
    let spin = Spinner::new(MSG_INITIALIZING_DATABASES_SPINNER);
    let secrets = SecretsConfig::read_with_base_path(
        shell,
        chain_config
            .external_node_config_path
            .clone()
            .context(MSG_EXTERNAL_NODE_CONFIG_NOT_INITIALIZED)?,
    )?;
    let db_config = DatabaseConfig::from_url(
        secrets
            .database
            .as_ref()
            .context("")?
            .server_url
            .as_ref()
            .context("")?
            .expose_url(),
    )?;
    drop_db_if_exists(&db_config)
        .await
        .context(MSG_FAILED_TO_DROP_SERVER_DATABASE_ERR)?;
    init_db(&db_config).await?;
    recreate_rocksdb_dirs(
        shell,
        &chain_config.rocks_db_path,
        RocksDBDirOption::ExternalNode,
    )?;
    let path_to_server_migration = chain_config.link_to_code.join(SERVER_MIGRATIONS);
    migrate_db(shell, path_to_server_migration, &db_config.full_url()).await?;
    spin.finish();
    Ok(())
}
