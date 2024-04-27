use anyhow::Context;
use serde::{Deserialize, Serialize};
use zksync_basic_types::{Address, H256};
use zksync_config::{
    configs::{
        chain::{NetworkConfig, StateKeeperConfig},
        genesis::SharedBridge,
    },
    GenesisConfig,
};

use crate::{envy_load, FromEnv};

// For initializing genesis file from  env it's required to have an additional struct,
// because these data is not required as part of the current Contract Config
#[derive(Deserialize, Serialize, Debug, Clone)]
struct ContractsForGenesis {
    pub genesis_root: Option<H256>,
    pub genesis_rollup_leaf_index: Option<u64>,
    pub genesis_batch_commitment: Option<H256>,
    pub genesis_protocol_version: Option<u16>,
    pub fri_recursion_scheduler_level_vk_hash: H256,
    pub fri_recursion_node_level_vk_hash: H256,
    pub fri_recursion_leaf_level_vk_hash: H256,
    pub snark_wrapper_vk_hash: H256,
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

impl FromEnv for GenesisConfig {
    fn from_env() -> anyhow::Result<Self> {
        // Getting genesis from environmental variables is a temporary measure, that will be
        // re-implemented and for the sake of simplicity we combine values from different sources
        // #PLA-811
        let network_config = &NetworkConfig::from_env()?;
        let contracts_config = &ContractsForGenesis::from_env()?;
        let state_keeper = StateKeeperConfig::from_env()?;
        let shared_bridge = if let Some(state_transition_proxy_addr) =
            contracts_config.state_transition_proxy_addr
        {
            Some(SharedBridge {
                bridgehub_proxy_addr: contracts_config
                    .bridgehub_proxy_addr
                    .context("Must be specified with bridgehub_proxy_addr")?,
                state_transition_proxy_addr,
                transparent_proxy_admin_addr: contracts_config
                    .transparent_proxy_admin_addr
                    .context("Must be specified with transparent_proxy_admin_addr")?,
            })
        } else {
            None
        };

        #[allow(deprecated)]
        Ok(GenesisConfig {
            protocol_version: contracts_config.genesis_protocol_version,
            genesis_root_hash: contracts_config.genesis_root,
            rollup_last_leaf_index: contracts_config.genesis_rollup_leaf_index,
            genesis_commitment: contracts_config.genesis_batch_commitment,
            bootloader_hash: state_keeper.bootloader_hash,
            default_aa_hash: state_keeper.default_aa_hash,
            l1_chain_id: network_config.network.chain_id(),
            l2_chain_id: network_config.zksync_network_id,
            recursion_node_level_vk_hash: contracts_config.fri_recursion_node_level_vk_hash,
            recursion_leaf_level_vk_hash: contracts_config.fri_recursion_leaf_level_vk_hash,
            recursion_circuits_set_vks_hash: H256::zero(),
            recursion_scheduler_level_vk_hash: contracts_config.snark_wrapper_vk_hash,
            fee_account: state_keeper
                .fee_account_addr
                .context("Fee account required for genesis")?,
            shared_bridge,
            dummy_verifier: false,
            l1_batch_commit_data_generator_mode: state_keeper.l1_batch_commit_data_generator_mode,
        })
    }
}
