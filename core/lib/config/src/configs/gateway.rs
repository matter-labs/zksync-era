use zksync_basic_types::{web3::Bytes, Address};

use super::ContractsConfig;

/// Config that is only stored for the gateway chain.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct GatewayConfig {
    pub state_transition_proxy_addr: Address,
    pub state_transition_implementation_addr: Address,
    pub verifier_addr: Address,
    pub validator_timelock_addr: Address,
    pub admin_facet_addr: Address,
    pub mailbox_facet_addr: Address,
    pub executor_facet_addr: Address,
    pub getters_facet_addr: Address,
    pub diamond_init_addr: Address,
    pub genesis_upgrade_addr: Address,
    pub default_upgrade_addr: Address,
    pub multicall3_addr: Address,
    pub relayed_sl_da_validator: Address,
    pub validium_da_validator: Address,
    pub diamond_cut_data: Bytes,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct GatewayChainConfig {
    pub state_transition_proxy_addr: Address,
    pub validator_timelock_addr: Address,
    pub multicall3_addr: Address,
    pub diamond_proxy_addr: Address,
    // FIXME: there is no "governnace" for a chain, only an admin, we
    // need to figure out what we mean here
    pub chain_admin_addr: Option<Address>,
    pub governance_addr: Address,
    pub settlement_layer: u64,
}

impl GatewayChainConfig {
    pub fn from_gateway_and_chain_data(
        gateway_config: &GatewayConfig,
        diamond_proxy_addr: Address,
        l2_chain_admin_addr: Address,
        settlement_layer: u64,
    ) -> Self {
        Self {
            state_transition_proxy_addr: gateway_config.state_transition_proxy_addr,
            validator_timelock_addr: gateway_config.validator_timelock_addr,
            multicall3_addr: gateway_config.multicall3_addr,
            diamond_proxy_addr,
            chain_admin_addr: Some(l2_chain_admin_addr),
            governance_addr: l2_chain_admin_addr,
            settlement_layer,
        }
    }
}

impl From<ContractsConfig> for GatewayChainConfig {
    fn from(value: ContractsConfig) -> Self {
        Self {
            state_transition_proxy_addr: value
                .ecosystem_contracts
                .unwrap()
                .state_transition_proxy_addr,
            validator_timelock_addr: value.validator_timelock_addr,
            multicall3_addr: value.l1_multicall3_addr,
            diamond_proxy_addr: value.diamond_proxy_addr,
            chain_admin_addr: value.chain_admin_addr,
            governance_addr: value.governance_addr,
            settlement_layer: value.settlement_layer.unwrap(),
        }
    }
}
