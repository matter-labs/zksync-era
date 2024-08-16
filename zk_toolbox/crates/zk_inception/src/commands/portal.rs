use std::{collections::HashMap, path::PathBuf};

use anyhow::{anyhow, Context};
use common::{docker, ethereum, ethereum::TokenInfo, logger};
use config::{portal::*, traits::SaveConfig, ChainConfig, EcosystemConfig};
use ethers::types::Address;
use types::{BaseToken, L1Network};
use xshell::Shell;

use crate::{
    commands::args::PortalArgs,
    consts::{L2_BASE_TOKEN_ADDRESS, PORTAL_CONFIG_FILE, PORTAL_DOCKER_IMAGE, PORTAL_DOCKER_PORT},
    messages::{
        msg_portal_chain_not_initialized, msg_portal_failed_to_configure_chain,
        msg_portal_starting_on, MSG_FAILED_TO_START_PORTAL_ERR,
    },
};

pub async fn create_hyperchain_config(
    chain_config: &ChainConfig,
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
    let secrets_config = chain_config.get_secrets_config()?;
    let l1_rpc_url = secrets_config
        .l1
        .as_ref()
        .context("l1")?
        .l1_rpc_url
        .expose_str();
    let l1_network = if public_l1_network_id.is_none() {
        // Initialize non-public L1 network
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
    // Base token:
    let (base_token_addr, base_token_info) = if chain_config.base_token == BaseToken::eth() {
        (
            ethers::utils::hex::encode_prefixed(Address::zero()),
            TokenInfo {
                symbol: "ETH".to_string(),
                name: "Ether".to_string(),
                decimals: 18,
            },
        )
    } else {
        (
            ethers::utils::hex::encode_prefixed(chain_config.base_token.address),
            ethereum::get_token_info(chain_config.base_token.address, l1_rpc_url.to_string())
                .await?,
        )
    };
    // Tokens: add base token
    let tokens = vec![TokenConfig {
        address: L2_BASE_TOKEN_ADDRESS.to_string(),
        l1_address: Some(base_token_addr.to_string()),
        symbol: base_token_info.symbol,
        decimals: base_token_info.decimals,
        name: Some(base_token_info.name.to_string()),
    }];
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

pub async fn create_hyperchains_config(
    chain_configs: &[ChainConfig],
) -> anyhow::Result<HyperchainsConfigs> {
    let mut hyperchain_configs = Vec::new();
    for chain_config in chain_configs {
        match create_hyperchain_config(chain_config).await {
            Ok(config) => hyperchain_configs.push(config),
            Err(_) => logger::warn(msg_portal_failed_to_configure_chain(&chain_config.name)),
        }
    }
    // Ensure at least one HyperchainConfig is created
    if hyperchain_configs.is_empty() {
        Err(anyhow!(MSG_FAILED_TO_START_PORTAL_ERR))
    } else {
        Ok(HyperchainsConfigs(hyperchain_configs))
    }
}

pub async fn run(shell: &Shell, args: PortalArgs) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chains = ecosystem_config.list_of_chains();

    let mut chain_configs = Vec::new();
    for chain in chains {
        match ecosystem_config.load_chain(Some(chain.clone())) {
            Some(chain_config) => chain_configs.push(chain_config),
            None => {
                logger::warn(msg_portal_chain_not_initialized(&chain));
                continue;
            }
        }
    }
    if chain_configs.is_empty() {
        return Err(anyhow!(MSG_FAILED_TO_START_PORTAL_ERR));
    }

    let hyperchain_configs = create_hyperchains_config(&chain_configs).await?;
    let runtime_config = PortalRuntimeConfig {
        node_type: "hyperchain".to_string(),
        hyperchain_configs,
    };

    // Serialize and save runtime config to file
    let config_file_path = shell.current_dir().join(PORTAL_CONFIG_FILE);
    runtime_config.save(shell, &config_file_path)?;
    logger::info(format!(
        "Portal config saved to {}",
        config_file_path.display()
    ));

    logger::info(msg_portal_starting_on("127.0.0.1", args.port));
    run_portal(shell, &config_file_path, args.port)?;
    Ok(())
}

fn run_portal(shell: &Shell, config_file_path: &PathBuf, port: u16) -> anyhow::Result<()> {
    let config_file_abs_path = shell.current_dir().join(config_file_path);

    let port_mapping = format!("{}:{}", port, PORTAL_DOCKER_PORT);
    let volume_mapping = format!(
        "{}:/usr/src/app/dist/config.js",
        config_file_abs_path.display()
    );

    let mut docker_args = HashMap::new();
    docker_args.insert("--platform".to_string(), "linux/amd64".to_string());
    docker_args.insert("-p".to_string(), port_mapping);
    docker_args.insert("-v".to_string(), volume_mapping);

    docker::run(shell, PORTAL_DOCKER_IMAGE, docker_args)
        .with_context(|| MSG_FAILED_TO_START_PORTAL_ERR)?;
    Ok(())
}
