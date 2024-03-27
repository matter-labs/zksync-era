// External uses
use serde::Deserialize;
// Workspace uses
use zksync_basic_types::{Address, H256};

/// Data about deployed contracts.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ContractsConfig {
    pub genesis_root: Option<H256>,
    pub genesis_rollup_leaf_index: Option<u64>,
    pub genesis_batch_commitment: Option<H256>,
    pub genesis_protocol_version: Option<u16>,
    pub governance_addr: Address,
    pub mailbox_facet_addr: Address,
    pub executor_facet_addr: Address,
    pub admin_facet_addr: Address,
    pub getters_facet_addr: Address,
    pub verifier_addr: Address,
    pub diamond_init_addr: Address,
    pub diamond_upgrade_init_addr: Address,
    pub diamond_proxy_addr: Address,
    pub validator_timelock_addr: Address,
    pub genesis_tx_hash: H256,
    pub l1_erc20_bridge_proxy_addr: Address,
    pub l1_erc20_bridge_impl_addr: Address,
    pub l2_erc20_bridge_addr: Address,
    pub l1_weth_bridge_proxy_addr: Option<Address>,
    pub l2_weth_bridge_addr: Option<Address>,
    pub l1_allow_list_addr: Address,
    pub l2_testnet_paymaster_addr: Option<Address>,
    pub recursion_scheduler_level_vk_hash: H256,
    pub recursion_node_level_vk_hash: H256,
    pub recursion_leaf_level_vk_hash: H256,
    pub recursion_circuits_set_vks_hash: H256,
    pub l1_multicall3_addr: Address,
    pub fri_recursion_scheduler_level_vk_hash: H256,
    pub fri_recursion_node_level_vk_hash: H256,
    pub fri_recursion_leaf_level_vk_hash: H256,
    pub snark_wrapper_vk_hash: H256,

    // These contracts will be used after shared bridge integration.
    pub bridgehub_proxy_addr: Option<Address>,
    pub bridgehub_impl_addr: Option<Address>,
    pub state_transition_proxy_addr: Option<Address>,
    pub state_transition_impl_addr: Option<Address>,
    pub transparent_proxy_admin_addr: Option<Address>,
}

impl ContractsConfig {
    /// Creates a mock instance of `ContractsConfig` to be used in tests.
    /// No data in the created object is valid.
    /// Every contract address is set to a random but unique non-zero value.
    /// Same goes for hashes.
    pub fn for_tests() -> Self {
        Self {
            genesis_root: Some(H256::repeat_byte(0x01)),
            genesis_rollup_leaf_index: Some(26),
            mailbox_facet_addr: Address::repeat_byte(0x01),
            executor_facet_addr: Address::repeat_byte(0x02),
            admin_facet_addr: Address::repeat_byte(0x03),
            transparent_proxy_admin_addr: Some(Address::repeat_byte(0x04)),
            getters_facet_addr: Address::repeat_byte(0x05),
            verifier_addr: Address::repeat_byte(0x06),
            diamond_init_addr: Address::repeat_byte(0x07),
            diamond_upgrade_init_addr: Address::repeat_byte(0x08),
            diamond_proxy_addr: Address::repeat_byte(0x09),
            validator_timelock_addr: Address::repeat_byte(0x0a),
            genesis_tx_hash: H256::repeat_byte(0x01),
            l1_erc20_bridge_proxy_addr: Address::repeat_byte(0x0b),
            l1_erc20_bridge_impl_addr: Address::repeat_byte(0x0c),
            l2_erc20_bridge_addr: Address::repeat_byte(0x0d),
            l1_weth_bridge_proxy_addr: Some(Address::repeat_byte(0x0e)),
            l2_weth_bridge_addr: Some(Address::repeat_byte(0x0f)),
            l1_allow_list_addr: Address::repeat_byte(0x10),
            l2_testnet_paymaster_addr: Some(Address::repeat_byte(0x11)),
            recursion_scheduler_level_vk_hash: H256::repeat_byte(0x02),
            recursion_node_level_vk_hash: H256::repeat_byte(0x03),
            recursion_leaf_level_vk_hash: H256::repeat_byte(0x04),
            recursion_circuits_set_vks_hash: H256::repeat_byte(0x05),
            l1_multicall3_addr: Address::repeat_byte(0x12),
            fri_recursion_scheduler_level_vk_hash: H256::repeat_byte(0x06),
            fri_recursion_node_level_vk_hash: H256::repeat_byte(0x07),
            fri_recursion_leaf_level_vk_hash: H256::repeat_byte(0x08),
            governance_addr: Address::repeat_byte(0x13),
            snark_wrapper_vk_hash: H256::repeat_byte(0x09),
            bridgehub_proxy_addr: Some(Address::repeat_byte(0x14)),
            bridgehub_impl_addr: Some(Address::repeat_byte(0x15)),
            state_transition_proxy_addr: Some(Address::repeat_byte(0x16)),
            state_transition_impl_addr: Some(Address::repeat_byte(0x17)),
            genesis_batch_commitment: Some(H256::repeat_byte(0x17)),
            genesis_protocol_version: Some(22),
        }
    }
}
