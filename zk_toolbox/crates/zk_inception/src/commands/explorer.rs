use std::path::Path;

use anyhow::Context;
use common::{db, docker, logger, Prompt};
use config::{
    explorer::*,
    explorer_compose::*,
    traits::{ReadConfig, SaveConfig},
    AppsChainConfig, AppsChainExplorerConfig, AppsEcosystemConfig, ChainConfig, EcosystemConfig,
    ExplorerServicesConfig,
};
use slugify_rs::slugify;
use url::Url;
use xshell::Shell;

use crate::{
    consts::L2_BASE_TOKEN_ADDRESS,
    defaults::{
        generate_explorer_db_name, DATABASE_EXPLORER_URL, DEFAULT_EXPLORER_API_PORT,
        DEFAULT_EXPLORER_DATA_FETCHER_PORT, DEFAULT_EXPLORER_WORKER_PORT,
    },
    messages::{
        msg_explorer_db_name_prompt, msg_explorer_db_url_prompt,
        msg_explorer_initializing_database_for, msg_explorer_starting_on,
        MSG_EXPLORER_FAILED_TO_ALLOCATE_PORTS_ERR, MSG_EXPLORER_FAILED_TO_DROP_DATABASE_ERR,
    },
    utils::ports::{EcosystemPorts, EcosystemPortsScanner},
};

fn get_l2_rpc_url(chain_config: &ChainConfig) -> anyhow::Result<String> {
    // Get L2 RPC URL from general config
    let general_config = chain_config.get_general_config()?;
    let rpc_url = general_config
        .api_config
        .as_ref()
        .map(|api_config| &api_config.web3_json_rpc.http_url)
        .context("api_config")?;
    Ok(rpc_url.to_string())
}

async fn build_explorer_chain_config(
    chain_config: &ChainConfig,
    apps_chain_explorer_config: &AppsChainExplorerConfig,
) -> anyhow::Result<ExplorerChainConfig> {
    let l2_rpc_url = get_l2_rpc_url(chain_config)?;
    // Get Verification API URL from general config
    let general_config = chain_config.get_general_config()?;
    let verification_api_url = general_config
        .contract_verifier
        .as_ref()
        .map(|verifier| &verifier.url)
        .context("verification_url")?;
    // Build API URL
    let api_port = apps_chain_explorer_config.services.api_http_port;
    let api_url = format!("http://127.0.0.1:{}", api_port);

    // Build explorer chain config
    Ok(ExplorerChainConfig {
        name: chain_config.name.clone(),
        l2_network_name: chain_config.name.clone(),
        l2_chain_id: chain_config.chain_id.as_u64(),
        rpc_url: l2_rpc_url.to_string(),
        api_url: api_url.to_string(),
        base_token_address: L2_BASE_TOKEN_ADDRESS.to_string(),
        hostnames: Vec::new(),
        icon: "/images/icons/zksync-arrows.svg".to_string(),
        maintenance: false,
        published: true,
        bridge_url: None,
        l1_explorer_url: None,
        verification_api_url: Some(verification_api_url.to_string()),
    })
}

pub async fn create_explorer_chain_config(
    chain_config: &ChainConfig,
    apps_chain_config: &AppsChainExplorerConfig,
    shell: &Shell,
) -> anyhow::Result<ExplorerChainConfig> {
    let explorer_config = build_explorer_chain_config(chain_config, apps_chain_config).await?;
    let config_path =
        ExplorerChainConfig::get_config_path(&shell.current_dir(), &chain_config.name);
    explorer_config.save(shell, config_path)?;
    Ok(explorer_config)
}

pub async fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config: EcosystemConfig = EcosystemConfig::from_file(shell)?;
    let ecosystem_path = shell.current_dir();
    //  Keep track of allocated ports (initialized lazily)
    let mut ecosystem_ports: Option<EcosystemPorts> = None;

    // Get ecosystem level apps.yaml config
    let apps_config = AppsEcosystemConfig::read_or_create_default(shell)?;
    // What chains to run the explorer for
    let chains_enabled = apps_config
        .explorer
        .chains_enabled
        .unwrap_or_else(|| ecosystem_config.list_of_chains());

    // For each chain - initialize if needed or read previously created configs
    let mut explorer_chain_configs = Vec::new();
    let mut backend_configs = Vec::new();
    for chain_name in chains_enabled.iter() {
        let chain_config = ecosystem_config
            .load_chain(Some(chain_name.clone()))
            .ok_or_else(|| anyhow::anyhow!("Failed to load chain config for {}", chain_name))?;

        // Should initialize? Check if apps chain config exists
        let mut should_initialize = false;
        let apps_chain_config_path = AppsChainConfig::get_config_path(&ecosystem_path, &chain_name);
        let apps_chain_config = match AppsChainConfig::read(shell, &apps_chain_config_path) {
            Ok(config) => config,
            Err(_) => {
                should_initialize = true;
                // Initialize explorer database
                logger::info(msg_explorer_initializing_database_for(&chain_name));
                let db_config = fill_database_values_with_prompt(&chain_config);
                initialize_explorer_database(&db_config).await?;

                // Allocate ports for backend services
                let services_config =
                    allocate_explorer_services_ports(&ecosystem_path, &mut ecosystem_ports)
                        .context(MSG_EXPLORER_FAILED_TO_ALLOCATE_PORTS_ERR)?;

                // Build and save apps chain config
                let app_chain_config = AppsChainConfig {
                    explorer: AppsChainExplorerConfig {
                        database_url: db_config.full_url(),
                        services: services_config,
                    },
                };
                app_chain_config.save(shell, &apps_chain_config_path)?;
                app_chain_config
            }
        };

        let l2_rpc_url = get_l2_rpc_url(&chain_config)?;
        let l2_rpc_url = Url::parse(&l2_rpc_url).context("Failed to parse L2 RPC URL")?;

        // Build backend compose config for the explorer chain services
        let backend_compose_config =
            ExplorerBackendComposeConfig::new(chain_name, l2_rpc_url, &apps_chain_config.explorer)?;
        backend_configs.push(backend_compose_config);

        // Read or create explorer chain config (JSON)
        let explorer_chain_config_path =
            ExplorerChainConfig::get_config_path(&ecosystem_path, chain_name);
        let explorer_chain_config = match should_initialize {
            true => {
                create_explorer_chain_config(&chain_config, &apps_chain_config.explorer, shell)
                    .await?
            }
            false => match ExplorerChainConfig::read(shell, &explorer_chain_config_path) {
                Ok(config) => config,
                Err(_) => {
                    create_explorer_chain_config(&chain_config, &apps_chain_config.explorer, shell)
                        .await?
                }
            },
        };
        explorer_chain_configs.push(explorer_chain_config);
    }

    // Generate and save explorer runtime config (JS)
    let explorer_runtime_config = ExplorerRuntimeConfig::new(explorer_chain_configs);
    let explorer_runtime_config_path = ExplorerRuntimeConfig::get_config_path(&ecosystem_path);
    explorer_runtime_config.save(shell, &explorer_runtime_config_path)?;

    // Generate and save explorer docker compose
    let app_config = ExplorerAppServiceConfig {
        port: apps_config.explorer.http_port,
        config_path: explorer_runtime_config_path,
    };
    let explorer_compose_config = ExplorerComposeConfig::new(app_config, backend_configs)?;
    let explorer_compose_path = ExplorerComposeConfig::get_config_path(&ecosystem_path);
    explorer_compose_config.save(&shell, &explorer_compose_path)?;

    // Launch explorer using docker compose
    logger::info(format!(
        "Using generated docker compose file at {}",
        explorer_compose_path.display()
    ));
    logger::info(msg_explorer_starting_on(
        "127.0.0.1",
        apps_config.explorer.http_port,
    ));
    run_explorer(shell, &explorer_compose_path)?;
    Ok(())
}

fn run_explorer(shell: &Shell, explorer_compose_config_path: &Path) -> anyhow::Result<()> {
    if let Some(docker_compose_file) = explorer_compose_config_path.to_str() {
        docker::up(shell, docker_compose_file, false)?;
    } else {
        anyhow::bail!("Invalid docker compose file");
    }
    Ok(())
}

fn fill_database_values_with_prompt(config: &ChainConfig) -> db::DatabaseConfig {
    let defaul_db_name: String = generate_explorer_db_name(config);
    let chain_name = config.name.clone();
    let explorer_db_url = Prompt::new(&msg_explorer_db_url_prompt(&chain_name))
        .default(DATABASE_EXPLORER_URL.as_str())
        .ask();
    let explorer_db_name: String = Prompt::new(&msg_explorer_db_name_prompt(&chain_name))
        .default(&defaul_db_name)
        .ask();
    let explorer_db_name = slugify!(&explorer_db_name, separator = "_");
    return db::DatabaseConfig::new(explorer_db_url, explorer_db_name);
}

pub async fn initialize_explorer_database(
    explorer_db_config: &db::DatabaseConfig,
) -> anyhow::Result<()> {
    db::drop_db_if_exists(explorer_db_config)
        .await
        .context(MSG_EXPLORER_FAILED_TO_DROP_DATABASE_ERR)?;
    db::init_db(explorer_db_config).await?;
    Ok(())
}

fn allocate_explorer_services_ports(
    ecosystem_path: &Path,
    ecosystem_ports: &mut Option<EcosystemPorts>,
) -> anyhow::Result<ExplorerServicesConfig> {
    let default_ports = vec![
        DEFAULT_EXPLORER_API_PORT,
        DEFAULT_EXPLORER_DATA_FETCHER_PORT,
        DEFAULT_EXPLORER_WORKER_PORT,
    ];

    // Get or scan ecosystem folder configs to find assigned ports
    let ports = match ecosystem_ports {
        Some(ref mut res) => res,
        None => ecosystem_ports.get_or_insert(
            EcosystemPortsScanner::scan(&ecosystem_path)
                .context("Failed to scan ecosystem ports")?,
        ),
    };

    // Try to allocate intuitive ports with an offset from the defaults
    let allocated = match ports.allocate_ports(&default_ports, 3001..4000, 100) {
        Ok(allocated) => allocated,
        Err(_) => {
            // Allocate one by one
            let mut allocated = Vec::new();
            for _ in 0..3 {
                let port = ports.allocate_port(3001..4000)?;
                allocated.push(port);
            }
            allocated
        }
    };

    // Build the explorer services config
    let services_config = ExplorerServicesConfig {
        api_http_port: allocated[0],
        data_fetcher_http_port: allocated[1],
        worker_http_port: allocated[2],
        batches_processing_polling_interval: None,
    };
    Ok(services_config.with_defaults())
}
