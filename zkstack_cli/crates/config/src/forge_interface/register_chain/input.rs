use ethers::{core::k256::U256, types::Address};
use rand::Rng;
use serde::{Deserialize, Serialize};
use types::L1BatchCommitmentMode;
use zksync_basic_types::L2ChainId;

use crate::{traits::ZkStackConfig, ChainConfig, ContractsConfig};

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Bridgehub {
    bridgehub_proxy_addr: Address,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct StateTransition {
    state_transition_proxy_addr: Address,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct DeployedAddresses {
    state_transition: StateTransition,
    bridgehub: Bridgehub,
    validator_timelock_addr: Address,
    chain_registrar: Address,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Contracts {
    diamond_cut_data: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RegisterChainL1Config {
    contracts_config: Contracts,
    deployed_addresses: DeployedAddresses,
    chain: ChainL1Config,
    owner_address: Address,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ChainL1Config {
    pub chain_chain_id: L2ChainId,
    pub proposal_author: Address,
    pub bridgehub_create_new_chain_salt: u64,
}

impl ZkStackConfig for RegisterChainL1Config {}

impl RegisterChainL1Config {
    pub fn new(chain_config: &ChainConfig, contracts: &ContractsConfig) -> anyhow::Result<Self> {
        let wallets_config = chain_config.get_wallets_config()?;
        Ok(Self {
            contracts_config: Contracts {
                diamond_cut_data: contracts.ecosystem_contracts.diamond_cut_data.clone(),
            },
            deployed_addresses: DeployedAddresses {
                state_transition: StateTransition {
                    state_transition_proxy_addr: contracts
                        .ecosystem_contracts
                        .state_transition_proxy_addr,
                },
                bridgehub: Bridgehub {
                    bridgehub_proxy_addr: contracts.ecosystem_contracts.bridgehub_proxy_addr,
                },
                validator_timelock_addr: contracts.ecosystem_contracts.validator_timelock_addr,
                chain_registrar: contracts.ecosystem_contracts.chain_registrar,
            },
            chain: ChainL1Config {
                chain_chain_id: chain_config.chain_id,
                proposal_author: wallets_config.governor.address,
                bridgehub_create_new_chain_salt: rand::thread_rng().gen_range(0..=i64::MAX) as u64,
            },
            owner_address: wallets_config.governor.address,
        })
    }
}
