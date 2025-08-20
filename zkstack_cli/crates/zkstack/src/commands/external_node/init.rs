use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{
    db::{drop_db_if_exists, init_db, migrate_db, DatabaseConfig},
    spinner::Spinner,
};
use zkstack_cli_config::{ChainConfig, EcosystemConfig, SecretsConfig, SECRETS_FILE};

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

    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    init(shell, &chain_config).await
}

pub async fn init(shell: &Shell, chain_config: &ChainConfig) -> anyhow::Result<()> {
    let spin = Spinner::new(MSG_INITIALIZING_DATABASES_SPINNER);
    let secrets_path = chain_config
        .external_node_config_path
        .as_ref()
        .context(MSG_EXTERNAL_NODE_CONFIG_NOT_INITIALIZED)?
        .join(SECRETS_FILE);
    let secrets = SecretsConfig::read(shell, &secrets_path).await?;
    let db_url = secrets
        .core_database_url()?
        .context("missing core database URL")?;
    let db_config = DatabaseConfig::from_url(&db_url)?;
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
    migrate_db(shell, &path_to_server_migration, &db_config.full_url()).await?;
    spin.finish();
    Ok(())
}
