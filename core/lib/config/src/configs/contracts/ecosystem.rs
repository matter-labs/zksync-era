use zksync_basic_types::Address;

/// Ecosystem contracts that are specific only for L1
#[derive(Debug, Clone)]
pub struct L1SpecificContracts {
    pub bytecodes_supplier_addr: Option<Address>,
    // Note that on the contract side of things this contract is called `L2WrappedBaseTokenStore`,
    // while on the server side for consistency with the conventions, where the prefix denotes
    // the location of the contracts we call it `l1_wrapped_base_token_store`
    pub wrapped_base_token_store: Option<Address>,
    pub bridge_hub: Option<Address>,
    pub shared_bridge: Option<Address>,
    pub erc_20_bridge: Option<Address>,
    pub base_token_address: Address,
    pub chain_admin: Option<Address>,
    pub server_notifier_addr: Option<Address>,
}

/// Ecosystem contracts that are presented on all Settlement Layers.
#[derive(Debug, Clone)]
pub struct EcosystemCommonContracts {
    pub bridgehub_proxy_addr: Option<Address>,
    pub state_transition_proxy_addr: Option<Address>,
    pub message_root_proxy_addr: Option<Address>,
    pub multicall3: Option<Address>,
    pub validator_timelock_addr: Option<Address>,
}
