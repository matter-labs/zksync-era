use anyhow::{anyhow, Context};
use common::{docker, logger};
use config::{ChainConfig, EcosystemConfig};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::io::Write;
use std::fs::File;
use std::path::PathBuf;
use types::L1Network;
use xshell::Shell;

use crate::{
    commands::args::PortalArgs,
    consts::{L2_BASE_TOKEN_ADDRESS, PORTAL_CONFIG_FILE, PORTAL_DOCKER_IMAGE, PORTAL_DOCKER_PORT},
    messages::{MSG_CHAINS_NOT_INITIALIZED, MSG_FAILED_TO_START_PORTAL_ERR, MSG_STARTING_PORTAL},
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PortalRuntimeConfig {
    #[serde(rename = "nodeType")]
    pub node_type: String,
    #[serde(rename = "hyperchainsConfig")]
    pub hyperchain_configs: HyperchainsConfigs,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HyperchainsConfigs(pub Vec<HyperchainConfig>);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HyperchainConfig {
    pub network: NetworkConfig,
    pub tokens: Vec<TokenConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NetworkConfig {
    pub id: u64,  // L2 Network ID
    pub key: String,  // L2 Network key
    pub name: String,  // L2 Network name
    #[serde(rename = "rpcUrl")]
    pub rpc_url: String,  // L2 RPC URL
    #[serde(rename = "blockExplorerUrl", skip_serializing_if = "Option::is_none")]
    pub block_explorer_url: Option<String>,  // L2 Block Explorer URL
    #[serde(rename = "blockExplorerApi", skip_serializing_if = "Option::is_none")]
    pub block_explorer_api: Option<String>,  // L2 Block Explorer API
    #[serde(rename = "publicL1NetworkId", skip_serializing_if = "Option::is_none")]
    pub public_l1_network_id: Option<u64>,  // Ethereum Mainnet or Ethereum Sepolia Testnet ID
    #[serde(rename = "l1Network", skip_serializing_if = "Option::is_none")]
    pub l1_network: Option<L1NetworkConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct L1NetworkConfig {
    pub id: u64,
    pub name: String,
    pub network: String,
    #[serde(rename = "nativeCurrency")]
    pub native_currency: NativeCurrency,
    #[serde(rename = "rpcUrls")]
    pub rpc_urls: RpcUrls,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NativeCurrency {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RpcUrls {
    pub default: RpcUrlConfig,
    pub public: RpcUrlConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RpcUrlConfig {
    pub http: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TokenConfig {
    pub address: String,
    pub symbol: String,
    pub decimals: u8,
    #[serde(rename = "l1Address", skip_serializing_if = "Option::is_none")]
    pub l1_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

pub fn create_hyperchain_config(
    ecosystem_config: &EcosystemConfig,
    chain_config: &ChainConfig
) -> anyhow::Result<HyperchainConfig> {
    // Get L2 RPC URL from general config
    let general_config = chain_config.get_general_config()?;
    let rpc_url = &general_config
        .api_config
        .as_ref()
        .context("api_config")?
        .web3_json_rpc
        .http_url;
    // Check if L1 network is public
    let public_l1_network_id = match chain_config.l1_network {
        L1Network::Sepolia | L1Network::Mainnet => Some(chain_config.l1_network.chain_id()),
        _ => None,
    };
    let l1_network = if public_l1_network_id.is_none() {
        // Initialize non-public L1 network
        let secrets_config = chain_config.get_secrets_config()?;
        let l1_rpc_url = secrets_config.l1.as_ref().context("l1")?.l1_rpc_url.expose_str();
        Some(L1NetworkConfig {
            id: chain_config.l1_network.chain_id(),
            name: chain_config.l1_network.to_string(),
            network: chain_config.l1_network.to_string().to_lowercase(),
            native_currency: NativeCurrency {
                name: "Ether".to_string(),
                symbol: "ETH".to_string(),
                decimals: 18,
            },
            rpc_urls: RpcUrls {
                default: RpcUrlConfig {
                    http: vec![l1_rpc_url.to_string()],
                },
                public: RpcUrlConfig {
                    http: vec![l1_rpc_url.to_string()],
                },
            },
        })
    } else {
        None
    };
    // TODO: use bridge contract to derive base token
    // TODO: add ERC20 tokens
    // Tokens: add ETH token
    let tokens = vec![
        TokenConfig {
            address: L2_BASE_TOKEN_ADDRESS.to_string(),
            l1_address: Some("0x0000000000000000000000000000000000000000".to_string()),
            symbol: "ETH".to_string(),
            decimals: 18,
            name: Some("Ether".to_string()),
        }
    ];
    // Build hyperchain config
    Ok(HyperchainConfig {
        network: NetworkConfig {
            id: chain_config.chain_id.as_u64(),
            key: chain_config.name.clone(),
            name: chain_config.name.clone(),
            rpc_url: rpc_url.to_string(),
            public_l1_network_id,
            l1_network,
            block_explorer_url: None,
            block_explorer_api: None,
        },
        tokens,
    })
}

pub fn create_hyperchains_config(
    ecosystem_config: &EcosystemConfig,
    chain_configs: &[ChainConfig]
) -> anyhow::Result<HyperchainsConfigs> {
    let mut hyperchain_configs = Vec::new();
    for chain_config in chain_configs {
        match create_hyperchain_config(ecosystem_config, chain_config) {
            Ok(config) => hyperchain_configs.push(config),
            Err(e) => logger::warn(format!("Failed to create HyperchainConfig: {}", e)),
        }
    }
    // Ensure at least one HyperchainConfig is created
    if hyperchain_configs.is_empty() {
        Err(anyhow!("Failed to create any valid HyperchainConfig"))
    } else {
        Ok(HyperchainsConfigs(hyperchain_configs))
    }
}

pub fn run(shell: &Shell, args: PortalArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chains = ecosystem_config.list_of_chains();

    let mut chain_configs = Vec::new();
    for chain in chains {
        match ecosystem_config.load_chain(Some(chain.clone())) {
            Some(chain_config) => chain_configs.push(chain_config),
            None => {
                logger::warn(format!("Chain '{}' is not initialized, skipping", chain));
                continue;
            }
        }
    }
    if chain_configs.is_empty() {
        return Err(anyhow!(MSG_CHAINS_NOT_INITIALIZED));
    }

    let hyperchain_configs = create_hyperchains_config(&ecosystem_config, &chain_configs)?;
    let runtime_config = PortalRuntimeConfig {
        node_type: "hyperchain".to_string(),
        hyperchain_configs,
    };

    // Serialize and save runtime config to file
    let json = serde_json::to_string_pretty(&runtime_config)
        .map_err(|e| anyhow!("Failed to serialize hyperchain config: {}", e))?;
    let config_js_content = format!("window['##runtimeConfig'] = {};", json);

    let config_file_path = shell.current_dir().join(PORTAL_CONFIG_FILE);
    let mut file = File::create(&config_file_path)
        .with_context(|| format!("Failed to create {}", PORTAL_CONFIG_FILE))?;
    file.write_all(config_js_content.as_bytes())
        .with_context(|| format!("Failed to write to {}", PORTAL_CONFIG_FILE))?;

    logger::info(format!("Config saved to {}", config_file_path.display()));

    logger::info(MSG_STARTING_PORTAL);
    run_portal(shell, &config_file_path, args.port)?;
    Ok(())
}

fn run_portal(
    shell: &Shell,
    config_file_path: &PathBuf,
    port: u16,
) -> anyhow::Result<()> {
    let config_file_abs_path = shell.current_dir().join(config_file_path);

    let port_mapping = format!("{}:{}", port, PORTAL_DOCKER_PORT);
    let volume_mapping = format!("{}:/usr/src/app/dist/config.js", config_file_abs_path.display());

    let mut docker_args = HashMap::new();
    docker_args.insert("-p".to_string(), port_mapping);
    docker_args.insert("-v".to_string(), volume_mapping);

    docker::run(shell, PORTAL_DOCKER_IMAGE, docker_args)
        .with_context(|| MSG_FAILED_TO_START_PORTAL_ERR)?;
    Ok(())
}
