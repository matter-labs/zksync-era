// External uses
use serde::Deserialize;
// Workspace uses
use zksync_basic_types::{Address, H256};

/// Data about deployed contracts.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ContractsConfig {
    pub governance_addr: Address,
    pub verifier_addr: Address,
    pub default_upgrade_addr: Address,
    pub diamond_proxy_addr: Address,
    pub validator_timelock_addr: Address,
    pub l1_erc20_bridge_proxy_addr: Address,
    pub l2_erc20_bridge_addr: Address,
    pub l1_weth_bridge_proxy_addr: Option<Address>,
    pub l2_weth_bridge_addr: Option<Address>,
    pub l2_testnet_paymaster_addr: Option<Address>,
    pub l1_multicall3_addr: Address,
}
