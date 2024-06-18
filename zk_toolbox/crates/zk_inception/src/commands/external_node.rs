use anyhow::Context;
use common::{
    config::global_config,
    db::{drop_db_if_exists, init_db, migrate_db, DatabaseConfig},
    logger,
};
use config::{traits::ReadConfigWithBasePath, ChainConfig, EcosystemConfig, SecretsConfig};
use xshell::Shell;

use crate::{
    commands::args::RunExternalNodeArgs,
    consts::SERVER_MIGRATIONS,
    external_node::RunExternalNode,
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_FAILED_TO_DROP_SERVER_DATABASE_ERR, MSG_STARTING_SERVER,
    },
};

pub async fn run(shell: &Shell, args: RunExternalNodeArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let chain = global_config().chain_name.clone();
    let chain_config = ecosystem_config
        .load_chain(chain)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    logger::info(MSG_STARTING_SERVER);

    run_external_node(args, &chain_config, shell).await?;

    Ok(())
}

async fn run_external_node(
    args: RunExternalNodeArgs,
    chain_config: &ChainConfig,
    shell: &Shell,
) -> anyhow::Result<()> {
    if args.reinit {
        let secrets = SecretsConfig::read_with_base_path(
            shell,
            chain_config
                .external_node_config_path
                .clone()
                .context("External node is not initalized")?,
        )?;
        let db_config = DatabaseConfig::from_url(secrets.database.server_url)?;
        drop_db_if_exists(&db_config)
            .await
            .context(MSG_FAILED_TO_DROP_SERVER_DATABASE_ERR)?;
        init_db(&db_config).await?;
        let path_to_server_migration = chain_config.link_to_code.join(SERVER_MIGRATIONS);
        migrate_db(shell, path_to_server_migration, &db_config.full_url()).await?;
    }
    let server = RunExternalNode::new(args.components.clone(), chain_config)?;
    server.run(shell, args.additional_args.clone())
}
