use ethers::types::Address;
use rand::Rng;
use serde::{Deserialize, Serialize};
use types::L1BatchCommitmentMode;
use zksync_basic_types::{web3::Bytes, L2ChainId};

use crate::{traits::ZkToolboxConfig, ChainConfig, ContractsConfig};

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
    native_token_vault_addr: Address,
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
    pub base_token_addr: Address,
    pub bridgehub_create_new_chain_salt: u64,
    pub validium_mode: bool,
    pub validator_sender_operator_commit_eth: Address,
    pub validator_sender_operator_blobs_eth: Address,
    pub base_token_gas_price_multiplier_nominator: u64,
    pub base_token_gas_price_multiplier_denominator: u64,
    pub governance_security_council_address: Address,
    pub governance_min_delay: u64,
    pub force_deployments_data: Bytes,
}

impl ZkToolboxConfig for RegisterChainL1Config {}

impl RegisterChainL1Config {
    pub fn new(chain_config: &ChainConfig, contracts: &ContractsConfig) -> anyhow::Result<Self> {
        let genesis_config = chain_config.get_genesis_config()?;
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
                native_token_vault_addr: contracts.ecosystem_contracts.native_token_vault_addr,
            },
            chain: ChainL1Config {
                chain_chain_id: genesis_config.l2_chain_id,
                base_token_gas_price_multiplier_nominator: chain_config.base_token.nominator,
                base_token_gas_price_multiplier_denominator: chain_config.base_token.denominator,
                base_token_addr: chain_config.base_token.address,
                // TODO specify
                governance_security_council_address: Default::default(),
                governance_min_delay: 0,
                // TODO verify
                bridgehub_create_new_chain_salt: rand::thread_rng().gen_range(0..=i64::MAX) as u64,
                validium_mode: chain_config.l1_batch_commit_data_generator_mode
                    == L1BatchCommitmentMode::Validium,
                validator_sender_operator_commit_eth: wallets_config.operator.address,
                validator_sender_operator_blobs_eth: wallets_config.blob_operator.address,
                force_deployments_data: contracts.l1.force_deployments_data.clone(),
            },
            owner_address: wallets_config.governor.address,
        })
    }
}
