use anyhow::Context;
use common::{config::global_config, db, logger, Prompt};
use config::{
    explorer::*,
    traits::{ConfigWithL2RpcUrl, ReadConfig, SaveConfig},
    AppsChainConfig, AppsChainExplorerConfig, ChainConfig, EcosystemConfig, ExplorerServicesConfig,
};
use slugify_rs::slugify;
use url::Url;
use xshell::Shell;

use crate::{
    commands::chain::args::init::PortOffset,
    consts::L2_BASE_TOKEN_ADDRESS,
    defaults::{
        generate_explorer_db_name, DATABASE_EXPLORER_URL, EXPLORER_API_PORT,
        EXPLORER_DATA_FETCHER_PORT, EXPLORER_WORKER_PORT,
    },
    messages::{
        msg_chain_load_err, msg_explorer_db_name_prompt, msg_explorer_db_url_prompt,
        msg_explorer_initializing_database_for, MSG_EXPLORER_FAILED_TO_DROP_DATABASE_ERR,
        MSG_EXPLORER_INITIALIZED,
    },
};

pub(crate) async fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    // If specific chain is provided, initialize only that chain; otherwise, initialize all chains
    let chains_enabled = match global_config().chain_name {
        Some(ref chain_name) => vec![chain_name.clone()],
        None => ecosystem_config.list_of_chains(),
    };
    // Initialize chains one by one
    for chain_name in chains_enabled.iter() {
        // Load chain config
        let chain_config = ecosystem_config
            .load_chain(Some(chain_name.clone()))
            .context(msg_chain_load_err(chain_name))?;
        // Initialize chain-level apps.yaml
        let apps_chain_config = initialize_apps_chain_config(shell, &chain_config)?;
        // Initialize explorer database
        initialize_explorer_database(&apps_chain_config.explorer.database_url).await?;
        // Create chain-level explorer.json
        create_explorer_chain_config(shell, &chain_config, &apps_chain_config.explorer)?;
    }
    logger::outro(MSG_EXPLORER_INITIALIZED);
    Ok(())
}

fn initialize_apps_chain_config(
    shell: &Shell,
    chain_config: &ChainConfig,
) -> anyhow::Result<AppsChainConfig> {
    let ecosystem_path = shell.current_dir();
    let apps_chain_config_path =
        AppsChainConfig::get_config_path(&ecosystem_path, &chain_config.name);
    // Check if apps chain config exists
    if let Ok(apps_chain_config) = AppsChainConfig::read(shell, &apps_chain_config_path) {
        return Ok(apps_chain_config);
    }
    // Prompt explorer database name
    logger::info(msg_explorer_initializing_database_for(&chain_config.name));
    let db_config = fill_database_values_with_prompt(chain_config);

    // Allocate ports for backend services
    let services_config = allocate_explorer_services_ports(chain_config);

    // Build and save apps chain config
    let app_chain_config = AppsChainConfig {
        explorer: AppsChainExplorerConfig {
            database_url: db_config.full_url(),
            services: services_config,
        },
    };
    app_chain_config.save(shell, &apps_chain_config_path)?;
    Ok(app_chain_config)
}

async fn initialize_explorer_database(db_url: &Url) -> anyhow::Result<()> {
    let db_config = db::DatabaseConfig::from_url(db_url)?;
    db::drop_db_if_exists(&db_config)
        .await
        .context(MSG_EXPLORER_FAILED_TO_DROP_DATABASE_ERR)?;
    db::init_db(&db_config).await?;
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
    db::DatabaseConfig::new(explorer_db_url, explorer_db_name)
}

fn allocate_explorer_services_ports(chain_config: &ChainConfig) -> ExplorerServicesConfig {
    // Try to allocate intuitive ports with an offset from the defaults
    let offset: u16 = PortOffset::from_chain_id(chain_config.id as u16).into();
    ExplorerServicesConfig::new(
        EXPLORER_API_PORT + offset,
        EXPLORER_DATA_FETCHER_PORT + offset,
        EXPLORER_WORKER_PORT + offset,
    )
}

fn create_explorer_chain_config(
    shell: &Shell,
    chain_config: &ChainConfig,
    apps_chain_config: &AppsChainExplorerConfig,
) -> anyhow::Result<ExplorerChainConfig> {
    let explorer_config = build_explorer_chain_config(chain_config, apps_chain_config)?;
    let config_path =
        ExplorerChainConfig::get_config_path(&shell.current_dir(), &chain_config.name);
    explorer_config.save(shell, config_path)?;
    Ok(explorer_config)
}

fn build_explorer_chain_config(
    chain_config: &ChainConfig,
    apps_chain_explorer_config: &AppsChainExplorerConfig,
) -> anyhow::Result<ExplorerChainConfig> {
    let general_config = chain_config.get_general_config()?;
    // Get L2 RPC URL from general config
    let l2_rpc_url = general_config.get_l2_rpc_url()?;
    // Get Verification API URL from general config
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
