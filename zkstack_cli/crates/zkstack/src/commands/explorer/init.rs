use anyhow::Context;
use clap::Parser;
use serde::{Deserialize, Serialize};
use slugify_rs::slugify;
use url::Url;
use xshell::Shell;
use zkstack_cli_common::{config::global_config, db, logger, Prompt, PromptConfirm};
use zkstack_cli_config::{
    explorer::{ExplorerChainConfig, ExplorerConfig},
    explorer_compose::{
        ExplorerBackendComposeConfig, ExplorerBackendConfig, ExplorerBackendPorts,
        PrividiumExplorerBackendConfig,
    },
    traits::SaveConfig,
    ChainConfig, EcosystemConfig, DEFAULT_PRIVATE_RPC_PORT, DEFAULT_PRIVATE_RPC_TOKEN_SECRET,
    DEFAULT_PRIVIDIUM_EXPLORER_SESSION_MAX_AGE, DEFAULT_PRIVIDIUM_EXPLORER_SESSION_SAME_SITE,
};

use crate::{
    consts::L2_BASE_TOKEN_ADDRESS,
    defaults::{generate_explorer_db_name, DATABASE_EXPLORER_URL},
    messages::{
        msg_chain_load_err, msg_explorer_db_name_prompt, msg_explorer_db_url_prompt,
        msg_explorer_initializing_database_for, MSG_EXPLORER_FAILED_TO_DROP_DATABASE_ERR,
        MSG_EXPLORER_INITIALIZED, MSG_EXPLORER_PRIVIDIUM_HELP, MSG_EXPLORER_PRIVIDIUM_MODE_PROMPT,
        MSG_EXPLORER_PRIVIDIUM_SESSION_MAX_AGE_PROMPT,
        MSG_EXPLORER_PRIVIDIUM_SESSION_SAME_SITE_PROMPT,
    },
    utils::ports::{EcosystemPorts, EcosystemPortsScanner},
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct ExplorerInitArgs {
    #[clap(long, help = MSG_EXPLORER_PRIVIDIUM_HELP, default_missing_value = "true", num_args = 0..=1)]
    pub prividium: Option<bool>,
}

pub(crate) async fn run(args: ExplorerInitArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    // If specific chain is provided, initialize only that chain; otherwise, initialize all chains
    let chains_enabled = match global_config().chain_name {
        Some(ref chain_name) => vec![chain_name.clone()],
        None => ecosystem_config.list_of_chains(),
    };
    // Initialize chains one by one
    let mut explorer_config = ExplorerConfig::read_or_create_default(shell)?;
    for chain_name in chains_enabled.iter() {
        // Load chain config
        let chain_config = ecosystem_config
            .load_chain(Some(chain_name.clone()))
            .context(msg_chain_load_err(chain_name))?;
        let mut ports = EcosystemPortsScanner::scan(shell, Some(&chain_config.name))?;
        // Build backend config - parameters required to create explorer backend services
        let backend_config = build_backend_config(&mut ports, &chain_config, &args)?;
        // Initialize explorer database
        initialize_explorer_database(&backend_config.database_url).await?;
        // Create explorer backend docker compose file
        let l2_rpc_url = chain_config.get_general_config().await?.l2_http_url()?;
        let l2_rpc_url = l2_rpc_url.parse().context("invalid L2 RPC URL")?;
        let backend_compose_config =
            ExplorerBackendComposeConfig::new(chain_name, l2_rpc_url, &backend_config)?;
        let backend_compose_config_path =
            ExplorerBackendComposeConfig::get_config_path(&shell.current_dir(), chain_name);
        backend_compose_config.save(shell, &backend_compose_config_path)?;
        // Add chain to explorer.json
        let explorer_chain_config =
            build_explorer_chain_config(&chain_config, &backend_config).await?;
        explorer_config.add_chain_config(&explorer_chain_config);
    }
    // Save explorer config
    let config_path = ExplorerConfig::get_config_path(&shell.current_dir());
    explorer_config.save(shell, config_path)?;

    logger::outro(MSG_EXPLORER_INITIALIZED);
    Ok(())
}

fn build_backend_config(
    ports: &mut EcosystemPorts,
    chain_config: &ChainConfig,
    args: &ExplorerInitArgs,
) -> anyhow::Result<ExplorerBackendConfig> {
    // Prompt explorer database name
    logger::info(msg_explorer_initializing_database_for(&chain_config.name));
    let db_config = fill_database_values_with_prompt(chain_config);

    // Allocate ports for backend services
    let mut backend_ports = ExplorerBackendPorts::default();
    ports.allocate_ports_with_offset_from_defaults(&mut backend_ports, chain_config.id)?;

    // Build prividium backend config if enabled
    let prividium_config = fill_prividium_backend_config_with_prompt(chain_config, args);

    // Build explorer backend config
    Ok(ExplorerBackendConfig::new(
        db_config.full_url(),
        &backend_ports,
        prividium_config,
    ))
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

fn fill_prividium_backend_config_with_prompt(
    chain_config: &ChainConfig,
    args: &ExplorerInitArgs,
) -> Option<PrividiumExplorerBackendConfig> {
    let prividium: bool = args.prividium.unwrap_or_else(|| {
        PromptConfirm::new(MSG_EXPLORER_PRIVIDIUM_MODE_PROMPT)
            .default(false)
            .ask()
    });
    if prividium {
        let session_max_age: u64 = Prompt::new(MSG_EXPLORER_PRIVIDIUM_SESSION_MAX_AGE_PROMPT)
            .default(&DEFAULT_PRIVIDIUM_EXPLORER_SESSION_MAX_AGE.to_string())
            .ask();
        let session_same_site: String =
            Prompt::new(MSG_EXPLORER_PRIVIDIUM_SESSION_SAME_SITE_PROMPT)
                .default(DEFAULT_PRIVIDIUM_EXPLORER_SESSION_SAME_SITE)
                .ask();

        Some(PrividiumExplorerBackendConfig::new(
            Url::parse(&format!("http://127.0.0.1:{DEFAULT_PRIVATE_RPC_PORT}")).unwrap(),
            DEFAULT_PRIVATE_RPC_TOKEN_SECRET.to_string(),
            chain_config.chain_id,
            session_max_age,
            session_same_site,
        ))
    } else {
        None
    }
}

async fn build_explorer_chain_config(
    chain_config: &ChainConfig,
    backend_config: &ExplorerBackendConfig,
) -> anyhow::Result<ExplorerChainConfig> {
    let general_config = chain_config.get_general_config().await?;
    let verification_api_url = general_config.contract_verifier_url()?;
    let api_port = backend_config.ports.api_http_port;
    let api_url: String = format!("http://127.0.0.1:{api_port}");
    let prividium = backend_config.prividium.is_some();
    let l2_rpc_url = if prividium {
        format!("http://127.0.0.1:{DEFAULT_PRIVATE_RPC_PORT}/rpc")
    } else {
        general_config.l2_http_url()?
    };

    // Build explorer chain config
    Ok(ExplorerChainConfig {
        name: chain_config.name.clone(),
        l2_network_name: chain_config.name.clone(),
        l2_chain_id: chain_config.chain_id.as_u64(),
        rpc_url: l2_rpc_url,
        api_url: api_url.to_string(),
        base_token_address: L2_BASE_TOKEN_ADDRESS.to_string(),
        hostnames: Vec::new(),
        icon: "/images/icons/zksync-arrows.svg".to_string(),
        maintenance: false,
        published: true,
        bridge_url: None,
        l1_explorer_url: None,
        verification_api_url: Some(verification_api_url.to_string()),
        prividium,
        other: serde_json::Value::Null,
    })
}
