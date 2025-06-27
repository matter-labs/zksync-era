use std::collections::HashMap;

use anyhow::Context;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use slugify_rs::slugify;
use url::Url;
use xshell::{cmd, Shell};
use zkstack_cli_common::{cmd::Cmd, db, docker, logger, Prompt};
use zkstack_cli_config::{
    docker_compose::{DockerComposeConfig, DockerComposeService},
    private_proxy_compose::{create_private_rpc_service, get_private_rpc_docker_compose_path},
    traits::SaveConfig,
    ChainConfig, ZkStackConfig, DEFAULT_PRIVATE_RPC_PORT, DEFAULT_PRIVATE_RPC_TOKEN_SECRET,
};

use crate::{
    defaults::{generate_private_rpc_db_name, DATABASE_PRIVATE_RPC_URL},
    messages::{
        msg_private_proxy_db_name_prompt, msg_private_rpc_chain_not_initialized,
        msg_private_rpc_db_url_prompt, msg_private_rpc_docker_compose_file_generated,
        msg_private_rpc_docker_image_being_built, msg_private_rpc_initializing_database_for,
        msg_private_rpc_permissions_file_generated, MSG_PRIVATE_RPC_FAILED_TO_DROP_DATABASE_ERR,
        MSG_PRIVATE_RPC_FAILED_TO_RUN_DOCKER_ERR,
    },
    utils::ports::{ConfigWithChainPorts, EcosystemPortsScanner},
};

#[derive(Debug, Clone, Serialize, Deserialize, Parser, Default)]
pub struct PrivateRpcCommandInitArgs {
    #[clap(long)]
    pub dev: bool,
    /// Initializes private proxy with network host mode. This is useful for environments that don't support host.docker.internal.
    #[clap(long)]
    pub docker_network_host: bool,
}

#[derive(Subcommand, Debug)]
pub enum PrivateRpcCommands {
    /// Initializes private proxy database, builds docker image, runs all migrations and creates docker-compose file
    Init(PrivateRpcCommandInitArgs),
    /// Run private proxy
    Run,
    /// Resets the private proxy database
    ResetDB,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PrivateProxyPorts {
    pub port: u16,
}

impl ConfigWithChainPorts for PrivateProxyPorts {
    fn get_default_ports(&self) -> anyhow::Result<HashMap<String, u16>> {
        let mut ports = HashMap::new();
        ports.insert("private-proxy".to_string(), DEFAULT_PRIVATE_RPC_PORT);
        Ok(ports)
    }

    fn set_ports(&mut self, ports: HashMap<String, u16>) -> anyhow::Result<()> {
        if let Some(port) = ports.get("private-proxy") {
            self.port = *port;
        } else {
            anyhow::bail!("Private proxy port not found");
        }
        Ok(())
    }
}

pub(crate) async fn run(shell: &Shell, args: PrivateRpcCommands) -> anyhow::Result<()> {
    match args {
        PrivateRpcCommands::Init(args) => init(shell, args).await,
        PrivateRpcCommands::Run => run_proxy(shell).await,
        PrivateRpcCommands::ResetDB => reset_db(shell).await,
    }
}

async fn reset_db(shell: &Shell) -> anyhow::Result<()> {
    let chain_config = ZkStackConfig::current_chain(shell)?;
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

pub async fn init(shell: &Shell, args: PrivateRpcCommandInitArgs) -> anyhow::Result<()> {
    let chain_config = ZkStackConfig::current_chain(shell)?;

    let chain_name = chain_config.name.clone();

    let _dir_guard = shell.push_dir(chain_config.link_to_code.join("private-rpc"));

    logger::info(msg_private_rpc_docker_image_being_built());
    Cmd::new(cmd!(
        shell,
        "docker build --platform linux/amd64 -t private-rpc ."
    ))
    .run()?;

    let db_config = if args.dev {
        let private_rpc_db_name: String = generate_private_rpc_db_name(&chain_config);
        db::DatabaseConfig::new(DATABASE_PRIVATE_RPC_URL.clone(), private_rpc_db_name).full_url()
    } else {
        prompt_db_config(&chain_config)?
    };

    initialize_private_rpc_database(shell, &chain_config, &db_config).await?;

    let src_permissions_path = "example-permissions.yaml";
    let dst_permissions_dir = &chain_config.abosulte_path_to_configs();
    let dst_permissions_path = dst_permissions_dir.join("private-rpc-permissions.yaml");

    if !dst_permissions_dir.exists() {
        shell
            .copy_file(src_permissions_path, &dst_permissions_path)
            .context("Failed to copy private RPC permissions file")?;
        logger::info(msg_private_rpc_permissions_file_generated(
            dst_permissions_path.display(),
        ));
    }

    let mut services: HashMap<String, DockerComposeService> = HashMap::new();
    let l2_rpc_url = chain_config.get_general_config().await?.l2_http_url()?;
    let l2_rpc_url = Url::parse(l2_rpc_url.as_str())?;

    let mut scanner = EcosystemPortsScanner::scan(shell, Some(&chain_config.name))?;
    let mut ports = PrivateProxyPorts::default();
    scanner.allocate_ports_with_offset_from_defaults(&mut ports, chain_config.id)?;

    services.insert(
        "private-proxy".to_string(),
        create_private_rpc_service(
            db_config,
            ports.port,
            DEFAULT_PRIVATE_RPC_TOKEN_SECRET,
            l2_rpc_url,
            &chain_config.configs,
            &chain_name,
            args.docker_network_host,
        )
        .await?,
    );

    let config = DockerComposeConfig {
        name: Some(format!("{chain_name}-private-proxy")),
        services,
        other: serde_json::Value::Null,
    };

    let docker_compose_path = get_private_rpc_docker_compose_path(&chain_config.configs);
    logger::info(msg_private_rpc_docker_compose_file_generated(
        docker_compose_path.display(),
    ));
    config.save(shell, docker_compose_path)?;

    Ok(())
}

pub async fn run_proxy(shell: &Shell) -> anyhow::Result<()> {
    let chain_config = ZkStackConfig::current_chain(shell)?;
    // Read chain-level private rpc docker compose file
    let backend_config_path = get_private_rpc_docker_compose_path(&chain_config.configs);
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
