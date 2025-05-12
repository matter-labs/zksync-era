use std::collections::HashMap;

use anyhow::Context;
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use slugify_rs::slugify;
use url::Url;
use xshell::{cmd, Shell};
use zkstack_cli_common::{
    cmd::Cmd, db, docker, docker::adjust_localhost_for_docker, logger, Prompt,
};
use zkstack_cli_config::{
    docker_compose::{DockerComposeConfig, DockerComposeService},
    private_proxy_compose::{create_private_rpc_service, get_private_rpc_docker_compose_path},
    traits::SaveConfig,
    ChainConfig, EcosystemConfig,
};

use crate::{
    defaults::{generate_private_rpc_db_name, DATABASE_PRIVATE_RPC_URL},
    messages::{
        msg_private_proxy_db_name_prompt, msg_private_rpc_chain_not_initialized,
        msg_private_rpc_db_url_prompt, msg_private_rpc_docker_compose_file_generated,
        msg_private_rpc_docker_image_being_built, msg_private_rpc_initializing_database_for,
        msg_private_rpc_permissions_file_generated, MSG_CHAIN_NOT_FOUND_ERR,
        MSG_CHAIN_NOT_INITIALIZED, MSG_PRIVATE_RPC_FAILED_TO_DROP_DATABASE_ERR,
        MSG_PRIVATE_RPC_FAILED_TO_RUN_DOCKER_ERR,
    },
};

#[derive(Subcommand, Debug)]
pub enum PrivateRpcCommands {
    /// Initializes private proxy database, builds docker image, runs all migrations and creates docker-compose file
    Init,
    /// Run private proxy
    Run,
    /// Resets the private proxy database
    ResetDB,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PrivateProxyConfig {
    pub database_url: Url,
    pub port: u16,
    pub create_token_secret: String,
}

pub(crate) async fn run(shell: &Shell, args: PrivateRpcCommands) -> anyhow::Result<()> {
    match args {
        PrivateRpcCommands::Init => init(shell).await,
        PrivateRpcCommands::Run => run_proxy(shell).await,
        PrivateRpcCommands::ResetDB => reset_db(shell).await,
    }
}

async fn reset_db(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    let db_url = prompt_db_config(&chain_config)?;
    let db_config = db::DatabaseConfig::from_url(&db_url)?;
    db::drop_db_if_exists(&db_config)
        .await
        .context(MSG_PRIVATE_RPC_FAILED_TO_DROP_DATABASE_ERR)?;
    initialize_private_rpc_database(shell, &chain_config, &db_url).await?;
    Ok(())
}

async fn initialize_private_rpc_database(
    shell: &Shell,
    chain_config: &ChainConfig,
    db_url: &Url,
) -> anyhow::Result<()> {
    let db_config = db::DatabaseConfig::from_url(db_url)?;
    let result = db::init_db(&db_config).await;
    if let Some(err) = result.err() {
        if !err.to_string().contains("already exists") {
            return Err(err);
        }
    }

    let db_url = db_config.full_url();
    let url_str = db_url.as_str();
    let migrations_dir = chain_config
        .link_to_code
        .join("private-rpc")
        .join("drizzle")
        .display()
        .to_string();
    Cmd::new(cmd!(
        shell,
        "cargo sqlx migrate run --database-url {url_str} --source {migrations_dir}"
    ))
    .run()?;

    Ok(())
}

fn prompt_db_config(config: &ChainConfig) -> anyhow::Result<Url> {
    let chain_name = config.name.clone();
    logger::info(msg_private_rpc_initializing_database_for(&chain_name));
    let default_db_name: String = generate_private_rpc_db_name(config);

    let private_rpc_db_url = Prompt::new(&msg_private_rpc_db_url_prompt(&chain_name))
        .default(DATABASE_PRIVATE_RPC_URL.as_str())
        .ask();
    let private_rpc_db_name: String = Prompt::new(&msg_private_proxy_db_name_prompt(&chain_name))
        .default(&default_db_name)
        .ask();
    let private_rpc_db_name = slugify!(&private_rpc_db_name, separator = "_");
    let db_config = db::DatabaseConfig::new(private_rpc_db_url, private_rpc_db_name);

    Ok(db_config.full_url())
}

pub async fn init(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let chain_name = chain_config.name.clone();

    let _dir_guard = shell.push_dir(chain_config.link_to_code.join("private-rpc"));

    logger::info(msg_private_rpc_docker_image_being_built());
    Cmd::new(cmd!(shell, "docker build -t private-rpc .")).run()?;

    let mut db_config = prompt_db_config(&chain_config)?;

    initialize_private_rpc_database(shell, &chain_config, &db_config).await?;
    db_config = adjust_localhost_for_docker(db_config)?;

    let mut services: HashMap<String, DockerComposeService> = HashMap::new();
    let l2_rpc_url = chain_config.get_general_config().await?.l2_http_url()?;
    let l2_rpc_url = Url::parse(l2_rpc_url.as_str())?;
    let l2_rpc_url = adjust_localhost_for_docker(l2_rpc_url)?;

    services.insert(
        "private-proxy".to_string(),
        create_private_rpc_service(db_config, 4041, "sososecret", l2_rpc_url).await?,
    );

    let config = DockerComposeConfig {
        name: Some(format!("{chain_name}-private-proxy")),
        services,
        other: serde_json::Value::Null,
    };

    let _dir_guard = shell.push_dir(&chain_config.link_to_code);
    let docker_compose_path =
        get_private_rpc_docker_compose_path(&shell.current_dir(), &chain_config.name);
    logger::info(msg_private_rpc_docker_compose_file_generated(
        docker_compose_path.display(),
    ));
    config.save(shell, docker_compose_path)?;

    let src_permissions_path = chain_config
        .link_to_code
        .join("private-rpc")
        .join("example-permissions.yaml");
    let dst_permissions_path = chain_config
        .link_to_code
        .join("chains")
        .join(chain_name.clone())
        .join("configs")
        .join("private-rpc-permissions.yaml");

    if !dst_permissions_path.exists() {
        Cmd::new(cmd!(
            shell,
            "cp {src_permissions_path} {dst_permissions_path}"
        ))
        .run()?;
        logger::info(msg_private_rpc_permissions_file_generated(
            dst_permissions_path.display(),
        ));
    }

    Ok(())
}

pub async fn run_proxy(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    // Read chain-level private rpc docker compose file
    let ecosystem_path = shell.current_dir();
    let backend_config_path =
        get_private_rpc_docker_compose_path(&ecosystem_path, &chain_config.name);
    if !backend_config_path.exists() {
        anyhow::bail!(msg_private_rpc_chain_not_initialized(&chain_config.name));
    }

    if let Some(docker_compose_file) = backend_config_path.to_str() {
        docker::up(shell, docker_compose_file, false)
            .context(MSG_PRIVATE_RPC_FAILED_TO_RUN_DOCKER_ERR)?;
    } else {
        anyhow::bail!("Invalid docker compose file");
    }
    Ok(())
}
