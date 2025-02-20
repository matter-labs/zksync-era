use anyhow::Context;
use serde::{Deserialize, Serialize};
use zksync_basic_types::{
    network::Network, protocol_version::ProtocolSemanticVersion, Address, L1ChainId, L2ChainId,
    H256,
};
use zksync_config::{configs::chain::StateKeeperConfig, GenesisConfig};

use crate::{envy_load, FromEnv};

/// Network-related env vars used to initialize the genesis config.
#[derive(Debug, Deserialize)]
struct NetworkConfig {
    network: Network,
    zksync_network_id: L2ChainId,
}

impl FromEnv for NetworkConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("network", "CHAIN_ETH_")
    }
}

// For initializing genesis file from  env it's required to have an additional struct,
// because these data is not required as part of the current Contract Config
#[derive(Deserialize, Serialize, Debug, Clone)]
struct ContractsForGenesis {
    pub genesis_root: Option<H256>,
    pub genesis_rollup_leaf_index: Option<u64>,
    pub genesis_batch_commitment: Option<H256>,
    pub genesis_protocol_version: Option<u16>,
    pub genesis_protocol_semantic_version: Option<ProtocolSemanticVersion>,
    pub fri_recursion_scheduler_level_vk_hash: H256,
    pub fri_recursion_node_level_vk_hash: H256,
    pub fri_recursion_leaf_level_vk_hash: H256,
    pub snark_wrapper_vk_hash: H256,
    pub fflonk_snark_wrapper_vk_hash: Option<H256>,
    // These contracts will be used after shared bridge integration.
    pub bridgehub_proxy_addr: Option<Address>,
    pub bridgehub_impl_addr: Option<Address>,
    pub state_transition_proxy_addr: Option<Address>,
    pub state_transition_impl_addr: Option<Address>,
    pub transparent_proxy_admin_addr: Option<Address>,
    pub l1_shared_bridge_proxy_addr: Option<Address>,
    pub l2_shared_bridge_addr: Option<Address>,
}

impl FromEnv for ContractsForGenesis {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("contracts_for_genesis", "CONTRACTS_")
    }
}

// For initializing genesis file from  env it's required to have an additional struct,
// because these data is not present in any other structs
#[derive(Deserialize, Serialize, Debug, Clone)]
struct CustomGenesisState {
    pub path: Option<String>,
}

impl FromEnv for CustomGenesisState {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("custom_genesis_state", "CUSTOM_GENESIS_STATE_")
    }
}

impl FromEnv for GenesisConfig {
    fn from_env() -> anyhow::Result<Self> {
        // Getting genesis from environmental variables is a temporary measure, that will be
        // re-implemented and for the sake of simplicity we combine values from different sources
        // #PLA-811
        let network_config = &NetworkConfig::from_env()?;
        let contracts_config = &ContractsForGenesis::from_env()?;
        let custom_genesis_state_config = CustomGenesisState::from_env()?;
        let state_keeper = StateKeeperConfig::from_env()?;

        // This is needed for backward compatibility, so if the new variable `genesis_protocol_semantic_version`
        // wasn't added yet server could still work. TODO: remove it in the next release.
        let protocol_version_deprecated = contracts_config
            .genesis_protocol_version
            .map(|minor| {
                minor.try_into().map(|minor| ProtocolSemanticVersion {
                    minor,
                    patch: 0.into(),
                })
            })
            .transpose()?;

        #[allow(deprecated)]
        Ok(GenesisConfig {
            protocol_version: contracts_config
                .genesis_protocol_semantic_version
                .or(protocol_version_deprecated),
            genesis_root_hash: contracts_config.genesis_root,
            rollup_last_leaf_index: contracts_config.genesis_rollup_leaf_index,
            genesis_commitment: contracts_config.genesis_batch_commitment,
            bootloader_hash: state_keeper.bootloader_hash,
            default_aa_hash: state_keeper.default_aa_hash,
            evm_emulator_hash: state_keeper.evm_emulator_hash,
            l1_chain_id: L1ChainId(network_config.network.chain_id().0),
            l2_chain_id: network_config.zksync_network_id,
            snark_wrapper_vk_hash: contracts_config.snark_wrapper_vk_hash,
            fflonk_snark_wrapper_vk_hash: contracts_config.fflonk_snark_wrapper_vk_hash,
            fee_account: state_keeper
                .fee_account_addr
                .context("Fee account required for genesis")?,
            dummy_verifier: false,
            l1_batch_commit_data_generator_mode: state_keeper.l1_batch_commit_data_generator_mode,
            custom_genesis_state_path: custom_genesis_state_config.path,
        })
    }
}
