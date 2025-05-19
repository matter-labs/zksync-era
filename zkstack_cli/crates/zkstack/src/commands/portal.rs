use std::path::Path;

use anyhow::Context;
use ethers::types::Address;
use xshell::Shell;
use zkstack_cli_common::{config::global_config, docker, ethereum, logger};
use zkstack_cli_config::{
    portal::*, traits::SaveConfig, AppsEcosystemConfig, ChainConfig, EcosystemConfig,
};
use zkstack_cli_types::{BaseToken, TokenInfo};

use crate::{
    consts::{L2_BASE_TOKEN_ADDRESS, PORTAL_DOCKER_CONFIG_PATH, PORTAL_DOCKER_IMAGE},
    messages::{
        msg_portal_running_with_config, msg_portal_starting_on,
        MSG_PORTAL_FAILED_TO_CREATE_CONFIG_ERR, MSG_PORTAL_FAILED_TO_FIND_ANY_CHAIN_ERR,
        MSG_PORTAL_FAILED_TO_RUN_DOCKER_ERR,
    },
};

async fn build_portal_chain_config(
    chain_config: &ChainConfig,
) -> anyhow::Result<PortalChainConfig> {
    // Get L2 RPC URL from general config
    let l2_rpc_url = chain_config.get_general_config().await?.l2_http_url()?;
    // Get L1 RPC URL from secrets config
    let secrets_config = chain_config.get_secrets_config().await?;
    let l1_rpc_url = secrets_config.l1_rpc_url()?;
    // Build L1 network config
    let l1_network = Some(L1NetworkConfig {
        id: chain_config.l1_network.chain_id(),
        name: chain_config.l1_network.to_string(),
        network: chain_config.l1_network.to_string().to_lowercase(),
        native_currency: TokenInfo::eth(),
        rpc_urls: RpcUrls {
            default: RpcUrlConfig {
                http: vec![l1_rpc_url.clone()],
            },
            public: RpcUrlConfig {
                http: vec![l1_rpc_url.clone()],
            },
        },
    });
    // Base token:
    let (base_token_addr, base_token_info) = if chain_config.base_token == BaseToken::eth() {
        (format!("{:?}", Address::zero()), TokenInfo::eth())
    } else {
        (
            format!("{:?}", chain_config.base_token.address),
            ethereum::get_token_info(chain_config.base_token.address, l1_rpc_url).await?,
        )
    };
    let tokens = vec![TokenConfig {
        address: L2_BASE_TOKEN_ADDRESS.to_string(),
        l1_address: Some(base_token_addr.to_string()),
        symbol: base_token_info.symbol,
        decimals: base_token_info.decimals,
        name: Some(base_token_info.name.to_string()),
    }];
    // Build hyperchain config
    Ok(PortalChainConfig {
        network: NetworkConfig {
            id: chain_config.chain_id.as_u64(),
            key: chain_config.name.clone(),
            name: chain_config.name.clone(),
            rpc_url: l2_rpc_url,
            l1_network,
            public_l1_network_id: None,
            block_explorer_url: None,
            block_explorer_api: None,
            hidden: None,
            other: serde_json::Value::Null,
        },
        tokens,
    })
}

pub async fn update_portal_config(
    shell: &Shell,
    chain_config: &ChainConfig,
) -> anyhow::Result<PortalConfig> {
    // Build and append portal chain config to the portal config
    let portal_chain_config = build_portal_chain_config(chain_config).await?;
    let mut portal_config = PortalConfig::read_or_create_default(shell)?;
    portal_config.add_chain_config(&portal_chain_config);
    // Save portal config
    let config_path = PortalConfig::get_config_path(&shell.current_dir());
    portal_config.save(shell, config_path)?;
    Ok(portal_config)
}

/// Validates portal config - appends missing chains and removes unknown chains
async fn validate_portal_config(
    portal_config: &mut PortalConfig,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<()> {
    let chain_names = ecosystem_config.list_of_chains();
    for chain_name in &chain_names {
        if portal_config.contains(chain_name) {
            continue;
        }
        // Append missing chain, chain might not be initialized, so ignoring errors
        if let Ok(chain_config) = ecosystem_config.load_chain(Some(chain_name.clone())) {
            if let Ok(portal_chain_config) = build_portal_chain_config(&chain_config).await {
                portal_config.add_chain_config(&portal_chain_config);
            }
        }
    }
    portal_config.filter(&chain_names);
    Ok(())
}

pub async fn run(shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config: EcosystemConfig = EcosystemConfig::from_file(shell)?;
    // Get ecosystem level apps.yaml config
    let apps_config = AppsEcosystemConfig::read_or_create_default(shell)?;
    // Display all chains, unless --chain is passed
    let chains_enabled = match global_config().chain_name {
        Some(ref chain_name) => vec![chain_name.clone()],
        None => ecosystem_config.list_of_chains(),
    };

    // Read portal config
    let config_path = PortalConfig::get_config_path(&shell.current_dir());
    let mut portal_config = PortalConfig::read_or_create_default(shell)
        .context(MSG_PORTAL_FAILED_TO_CREATE_CONFIG_ERR)?;

    // Validate and update portal config
    validate_portal_config(&mut portal_config, &ecosystem_config).await?;
    portal_config.hide_except(&chains_enabled);
    if portal_config.is_empty() {
        anyhow::bail!(MSG_PORTAL_FAILED_TO_FIND_ANY_CHAIN_ERR);
    }

    // Save portal config
    portal_config.save(shell, &config_path)?;

    let config_js_path = portal_config
        .save_as_js(shell)
        .context(MSG_PORTAL_FAILED_TO_CREATE_CONFIG_ERR)?;

    logger::info(msg_portal_running_with_config(&config_path));
    logger::info(msg_portal_starting_on(
        "127.0.0.1",
        apps_config.portal.http_port,
    ));
    let name = portal_app_name(&ecosystem_config.name);
    run_portal(shell, &config_js_path, &name, apps_config.portal.http_port)?;
    Ok(())
}

fn run_portal(shell: &Shell, config_file_path: &Path, name: &str, port: u16) -> anyhow::Result<()> {
    let port_mapping = format!("{}:{}", port, port);
    let volume_mapping = format!(
        "{}:{}",
        config_file_path.display(),
        PORTAL_DOCKER_CONFIG_PATH
    );

    let docker_args: Vec<String> = vec![
        "--platform".to_string(),
        "linux/amd64".to_string(),
        "--name".to_string(),
        name.to_string(),
        "-p".to_string(),
        port_mapping,
        "-v".to_string(),
        volume_mapping,
        "-e".to_string(),
        format!("PORT={}", port),
        "--rm".to_string(),
    ];

    docker::run(shell, PORTAL_DOCKER_IMAGE, docker_args)
        .with_context(|| MSG_PORTAL_FAILED_TO_RUN_DOCKER_ERR)?;
    Ok(())
}

/// Generates a name for the portal app Docker container.
/// Will be passed as `--name` argument to `docker run`.
fn portal_app_name(ecosystem_name: &str) -> String {
    format!("{}-portal-app", ecosystem_name)
}
