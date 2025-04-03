use zksync_basic_types::{Address, SLChainId};

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct GatewayChainConfig {
    pub state_transition_proxy_addr: Option<Address>,
    pub validator_timelock_addr: Option<Address>,
    pub multicall3_addr: Address,
    pub diamond_proxy_addr: Address,
    pub chain_admin_addr: Address,
    pub gateway_chain_id: SLChainId,
}
