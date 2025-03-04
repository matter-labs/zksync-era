// External uses
use serde::{Deserialize, Serialize};
// Workspace uses
use zksync_basic_types::Address;

// Unified ecosystem contracts. To be deleted, after contracts config migration
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct EcosystemContracts {
    pub bridgehub_proxy_addr: Address,
    pub state_transition_proxy_addr: Address,
    pub transparent_proxy_admin_addr: Address,
    pub l1_bytecodes_supplier_addr: Option<Address>,
    // Note that on the contract side of things this contract is called `L2WrappedBaseTokenStore`,
    // while on the server side for consistency with the conventions, where the prefix denotes
    // the location of the contracts we call it `l1_wrapped_base_token_store`
    pub l1_wrapped_base_token_store: Option<Address>,
    pub server_notifier_addr: Option<Address>,
}

impl EcosystemContracts {
    pub(crate) fn for_tests() -> Self {
        Self {
            bridgehub_proxy_addr: Address::repeat_byte(0x14),
            state_transition_proxy_addr: Address::repeat_byte(0x15),
            transparent_proxy_admin_addr: Address::repeat_byte(0x15),
            l1_bytecodes_supplier_addr: Some(Address::repeat_byte(0x16)),
            l1_wrapped_base_token_store: Some(Address::repeat_byte(0x17)),
            server_notifier_addr: Some(Address::repeat_byte(0x18)),
        }
    }
}

// Ecosystem contracts that are specific only for L1
#[derive(Debug, Clone)]
pub struct EcosystemL1Specific {
    pub bytecodes_supplier_addr: Option<Address>,
    // Note that on the contract side of things this contract is called `L2WrappedBaseTokenStore`,
    // while on the server side for consistency with the conventions, where the prefix denotes
    // the location of the contracts we call it `l1_wrapped_base_token_store`
    pub wrapped_base_token_store: Option<Address>,
    pub shared_bridge: Option<Address>,
    pub erc_20_bridge: Option<Address>,
    pub base_token_address: Option<Address>,
}

// Ecosystem contracts that are presented on all Settlement Layers
#[derive(Debug, Clone)]
pub struct EcosystemCommonContracts {
    pub bridgehub_proxy_addr: Address,
    pub state_transition_proxy_addr: Address,
    pub server_notifier_addr: Option<Address>,
    pub multicall3: Address,
    pub validator_timelock_addr: Address,
    pub no_da_validium_l1_validator_addr: Option<Address>,
}
