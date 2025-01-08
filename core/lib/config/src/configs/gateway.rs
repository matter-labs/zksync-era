use zksync_basic_types::{Address, SLChainId};

#[derive(Debug, Clone, PartialEq)]
pub struct GatewayChainConfig {
    pub state_transition_proxy_addr: Address,
    pub validator_timelock_addr: Address,
    pub multicall3_addr: Address,
    pub diamond_proxy_addr: Address,
    // TODO(EVM-921): there is no "governace" for a chain, only an admin, we
    // need to figure out what we mean here
    pub chain_admin_addr: Option<Address>,
    pub governance_addr: Address,
    pub gateway_chain_id: SLChainId,
}
