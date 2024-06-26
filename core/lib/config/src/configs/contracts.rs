// External uses
use serde::{Deserialize, Serialize};
// Workspace uses
use zksync_basic_types::Address;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct EcosystemContracts {
    pub bridgehub_proxy_addr: Address,
    pub state_transition_proxy_addr: Address,
    pub transparent_proxy_admin_addr: Address,
}

impl EcosystemContracts {
    fn for_tests() -> Self {
        Self {
            bridgehub_proxy_addr: Address::repeat_byte(0x14),
            state_transition_proxy_addr: Address::repeat_byte(0x15),
            transparent_proxy_admin_addr: Address::repeat_byte(0x15),
        }
    }
}

/// Data about deployed contracts.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ContractsConfig {
    pub governance_addr: Address,
    pub verifier_addr: Address,
    pub default_upgrade_addr: Address,
    pub diamond_proxy_addr: Address,
    pub validator_timelock_addr: Address,
    pub l1_shared_bridge_proxy_addr: Option<Address>,
    pub l2_shared_bridge_addr: Option<Address>,
    pub l1_erc20_bridge_proxy_addr: Option<Address>,
    pub l2_erc20_bridge_addr: Option<Address>,
    pub l1_weth_bridge_proxy_addr: Option<Address>,
    pub l2_weth_bridge_addr: Option<Address>,
    pub l2_testnet_paymaster_addr: Option<Address>,
    pub l1_multicall3_addr: Address,
    pub ecosystem_contracts: Option<EcosystemContracts>,
    // Used by the RPC API and by the BaseTokenAdjuster. If not set, ETH is the chain's base token
    // and the "ETH address" is SHARED_BRIDGE_ETHER_TOKEN_ADDRESS.
    pub base_token_addr: Address,
}

impl ContractsConfig {
    pub fn for_tests() -> Self {
        Self {
            verifier_addr: Address::repeat_byte(0x06),
            default_upgrade_addr: Address::repeat_byte(0x06),
            diamond_proxy_addr: Address::repeat_byte(0x09),
            validator_timelock_addr: Address::repeat_byte(0x0a),
            l1_erc20_bridge_proxy_addr: Some(Address::repeat_byte(0x0b)),
            l2_erc20_bridge_addr: Some(Address::repeat_byte(0x0c)),
            l1_shared_bridge_proxy_addr: Some(Address::repeat_byte(0x0e)),
            l2_shared_bridge_addr: Some(Address::repeat_byte(0x0f)),
            l1_weth_bridge_proxy_addr: Some(Address::repeat_byte(0x0b)),
            l2_weth_bridge_addr: Some(Address::repeat_byte(0x0c)),
            l2_testnet_paymaster_addr: Some(Address::repeat_byte(0x11)),
            l1_multicall3_addr: Address::repeat_byte(0x12),
            governance_addr: Address::repeat_byte(0x13),
            base_token_addr: Address::repeat_byte(0x14),
            ecosystem_contracts: Some(EcosystemContracts::for_tests()),
        }
    }
}
