use anyhow::anyhow;
use zksync_basic_types::H256;
use zksync_config::{
    configs::chain::{NetworkConfig, StateKeeperConfig},
    ContractsConfig, GenesisConfig,
};

use crate::FromEnv;

impl FromEnv for GenesisConfig {
    fn from_env() -> anyhow::Result<Self> {
        let network_config = &NetworkConfig::from_env()?;
        let contracts_config = &ContractsConfig::from_env()?;
        let state_keeper = StateKeeperConfig::from_env()?;
        Ok(GenesisConfig {
            protocol_version: 20,
            genesis_root_hash: contracts_config
                .genesis_root
                .ok_or(anyhow!("genesis_root_hash required for genesis"))?,
            rollup_last_leaf_index: contracts_config
                .genesis_rollup_leaf_index
                .ok_or(anyhow!("rollup_last_leaf_index required for genesis"))?,
            genesis_commitment: contracts_config
                .genesis_batch_commitment
                .ok_or(anyhow!("genesis_commitment required for genesis"))?,
            bootloader_hash: state_keeper
                .bootloader_hash
                .ok_or(anyhow!("Bootloader hash required for genesis"))?,
            default_aa_hash: state_keeper
                .default_aa_hash
                .ok_or(anyhow!("Default aa hash required for genesis"))?,
            verifier_address: contracts_config.verifier_addr,
            fee_account: state_keeper.fee_account_addr,
            diamond_proxy: contracts_config.diamond_proxy_addr,
            erc20_bridge: contracts_config.l1_erc20_bridge_proxy_addr,
            state_transition_proxy_addr: contracts_config.state_transition_proxy_addr,
            l1_chain_id: network_config.network.chain_id(),
            l2_chain_id: network_config.zksync_network_id,
            recursion_node_level_vk_hash: contracts_config.fri_recursion_node_level_vk_hash,
            recursion_leaf_level_vk_hash: contracts_config.fri_recursion_leaf_level_vk_hash,
            recursion_circuits_set_vks_hash: H256::zero(),
            recursion_scheduler_level_vk_hash: contracts_config.snark_wrapper_vk_hash,
        })
    }
}
