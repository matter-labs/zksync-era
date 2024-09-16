use zksync_basic_types::{web3::Bytes, Address};

/// Config that is only stored for the gateway chain.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct GatewayConfig {
    pub state_transition_proxy_addr: Address,
    pub state_transition_implementation_addr: Address,
    pub verifier_addr: Address,
    pub admin_facet_addr: Address,
    pub mailbox_facet_addr: Address,
    pub executor_facet_addr: Address,
    pub getters_facet_addr: Address,
    pub diamond_init_addr: Address,
    pub genesis_upgrade_addr: Address,
    pub default_upgrade_addr: Address,
    pub multicall3_addr: Address,
    pub diamond_cut_data: Bytes,
}
