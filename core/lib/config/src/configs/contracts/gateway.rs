use zksync_basic_types::{web3::Bytes, Address, SLChainId};

use super::chain::AllContractsConfig;

/// Config that is only stored for the gateway chain.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct GatewayConfig {
    pub state_transition_proxy_addr: Address,
    pub validator_timelock_addr: Address,
    pub multicall3_addr: Address,
    pub relayed_sl_da_validator: Address,
    pub validium_da_validator: Address,
    pub diamond_cut_data: Bytes,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct GatewayChainConfig {
    pub state_transition_proxy_addr: Option<Address>,
    pub validator_timelock_addr: Option<Address>,
    pub multicall3_addr: Address,
    pub diamond_proxy_addr: Address,
    pub gateway_chain_id: SLChainId,
}

impl GatewayChainConfig {
    pub fn from_gateway_and_chain_data(
        gateway_config: &GatewayConfig,
        diamond_proxy_addr: Address,
        gateway_chain_id: SLChainId,
    ) -> Self {
        Self {
            state_transition_proxy_addr: Some(gateway_config.state_transition_proxy_addr),
            validator_timelock_addr: Some(gateway_config.validator_timelock_addr),
            multicall3_addr: gateway_config.multicall3_addr,
            diamond_proxy_addr,
            gateway_chain_id,
        }
    }

    pub fn from_contracts_and_chain_id(
        contracts: AllContractsConfig,
        gateway_chain_id: SLChainId,
    ) -> Self {
        let sl_contracts = contracts.settlement_layer_specific_contracts();
        Self {
            state_transition_proxy_addr: sl_contracts
                .ecosystem_contracts
                .state_transition_proxy_addr,
            validator_timelock_addr: sl_contracts.ecosystem_contracts.validator_timelock_addr,
            multicall3_addr: sl_contracts.ecosystem_contracts.multicall3.unwrap(),
            diamond_proxy_addr: sl_contracts.chain_contracts_config.diamond_proxy_addr,
            gateway_chain_id,
        }
    }
}
