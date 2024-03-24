use anyhow::anyhow;
use zksync_config::{
    configs::chain::{NetworkConfig, StateKeeperConfig},
    ContractsConfig, GenesisConfig,
};

use crate::FromEnv;

impl FromEnv for GenesisConfig {
    fn from_env() -> anyhow::Result<Self> {
        // Getting genesis from environmental variables is a temporary measure, that will be
        // re-implemented and for the sake of simplicity we combine values from different sources
        // #PLA-811
        let network_config = &NetworkConfig::from_env()?;
        let contracts_config = &ContractsConfig::from_env()?;
        let state_keeper = StateKeeperConfig::from_env()?;
        Ok(GenesisConfig {
            protocol_version: contracts_config
                .genesis_protocol_version
                .ok_or(anyhow!("Protocol version is required for genesis"))?,
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
            fee_account: state_keeper.fee_account_addr,
            l1_chain_id: network_config.network.chain_id(),
            l2_chain_id: network_config.zksync_network_id,
            recursion_node_level_vk_hash: contracts_config.fri_recursion_node_level_vk_hash,
            recursion_leaf_level_vk_hash: contracts_config.fri_recursion_leaf_level_vk_hash,
            recursion_scheduler_level_vk_hash: contracts_config.snark_wrapper_vk_hash,
        })
    }
}
