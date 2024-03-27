use zksync_config::ContractsConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for ContractsConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("contracts", "CONTRACTS_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{addr, hash, EnvMutex};

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> ContractsConfig {
        ContractsConfig {
            genesis_root: Some(hash(
                "0x436cf80dd02a7e1a1df65be6ec9ea231ccec97c44f4c8c9cd2aa26c2feb074cd",
            )),
            genesis_rollup_leaf_index: Some(26),
            genesis_batch_commitment: Some(hash(
                "0x938016208176c5a49d47c8aa582b5d18afc4f159dfa099087770e0796948fd1a",
            )),
            bridgehub_proxy_addr: Some(addr("35ea7f92f4c5f433efe15284e99c040110cf6297")),
            bridgehub_impl_addr: Some(addr("87d456da9ed212eb49d80d96afb44afddf36adf8")),
            state_transition_proxy_addr: Some(addr("d90f1c081c6117241624e97cb6147257c3cb2097")),
            state_transition_impl_addr: Some(addr("c957c0e82d3bafb5ad46ffbcc66900648784eb05")),
            governance_addr: addr("d8dA6BF26964aF9D7eEd9e03E53415D37aA96045"),
            mailbox_facet_addr: addr("0f6Fa881EF414Fc6E818180657c2d5CD7Ac6cCAd"),
            executor_facet_addr: addr("18B631537801963A964211C0E86645c1aBfbB2d3"),
            admin_facet_addr: addr("1e12b20BE86bEc3A0aC95aA52ade345cB9AE7a32"),
            getters_facet_addr: addr("8656770FA78c830456B00B4fFCeE6b1De0e1b888"),
            verifier_addr: addr("34782eE00206EAB6478F2692caa800e4A581687b"),
            diamond_init_addr: addr("FFC35A5e767BE36057c34586303498e3de7C62Ba"),
            diamond_upgrade_init_addr: addr("FFC35A5e767BE36057c34586303498e3de7C62Ba"),
            diamond_proxy_addr: addr("F00B988a98Ca742e7958DeF9F7823b5908715f4a"),
            transparent_proxy_admin_addr: Some(addr("dd6fa5c14e7550b4caf2aa2818d24c69cbc347e5")),
            validator_timelock_addr: addr("F00B988a98Ca742e7958DeF9F7823b5908715f4a"),
            genesis_tx_hash: hash(
                "b99ebfea46cbe05a21cd80fe5597d97b204befc52a16303f579c607dc1ac2e2e",
            ),
            l1_erc20_bridge_proxy_addr: addr("8656770FA78c830456B00B4fFCeE6b1De0e1b888"),
            l1_erc20_bridge_impl_addr: addr("8656770FA78c830456B00B4fFCeE6b1De0e1b888"),
            l2_erc20_bridge_addr: addr("8656770FA78c830456B00B4fFCeE6b1De0e1b888"),
            l1_allow_list_addr: addr("8656770FA78c830456B00B4fFCeE6b1De0e1b888"),
            l1_weth_bridge_proxy_addr: Some(addr("8656770FA78c830456B00B4fFCeE6b1De0e1b888")),
            l2_weth_bridge_addr: Some(addr("8656770FA78c830456B00B4fFCeE6b1De0e1b888")),
            l2_testnet_paymaster_addr: Some(addr("FC073319977e314F251EAE6ae6bE76B0B3BAeeCF")),
            recursion_scheduler_level_vk_hash: hash(
                "0x1186ec268d49f1905f8d9c1e9d39fc33e98c74f91d91a21b8f7ef78bd09a8db8",
            ),
            recursion_node_level_vk_hash: hash(
                "0x1186ec268d49f1905f8d9c1e9d39fc33e98c74f91d91a21b8f7ef78bd09a8db8",
            ),
            recursion_leaf_level_vk_hash: hash(
                "0x101e08b00193e529145ee09823378ef51a3bc8966504064f1f6ba3f1ba863210",
            ),
            recursion_circuits_set_vks_hash: hash(
                "0x142a364ef2073132eaf07aa7f3d8495065be5b92a2dc14fda09b4216affed9c0",
            ),
            l1_multicall3_addr: addr("0xcA11bde05977b3631167028862bE2a173976CA11"),
            fri_recursion_scheduler_level_vk_hash: hash(
                "0x201d4c7d8e781d51a3bbd451a43a8f45240bb765b565ae6ce69192d918c3563d",
            ),
            fri_recursion_node_level_vk_hash: hash(
                "0x5a3ef282b21e12fe1f4438e5bb158fc5060b160559c5158c6389d62d9fe3d080",
            ),
            fri_recursion_leaf_level_vk_hash: hash(
                "0x72167c43a46cf38875b267d67716edc4563861364a3c03ab7aee73498421e828",
            ),
            snark_wrapper_vk_hash: hash(
                "0x4be443afd605a782b6e56d199df2460a025c81b3dea144e135bece83612563f2",
            ),
            genesis_protocol_version: Some(22),
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
CONTRACTS_GOVERNANCE_ADDR="0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045"
CONTRACTS_MAILBOX_FACET_ADDR="0x0f6Fa881EF414Fc6E818180657c2d5CD7Ac6cCAd"
CONTRACTS_EXECUTOR_FACET_ADDR="0x18B631537801963A964211C0E86645c1aBfbB2d3"
CONTRACTS_ADMIN_FACET_ADDR="0x1e12b20BE86bEc3A0aC95aA52ade345cB9AE7a32"
CONTRACTS_GETTERS_FACET_ADDR="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
CONTRACTS_VERIFIER_ADDR="0x34782eE00206EAB6478F2692caa800e4A581687b"
CONTRACTS_DIAMOND_INIT_ADDR="0xFFC35A5e767BE36057c34586303498e3de7C62Ba"
CONTRACTS_DIAMOND_UPGRADE_INIT_ADDR="0xFFC35A5e767BE36057c34586303498e3de7C62Ba"
CONTRACTS_DIAMOND_PROXY_ADDR="0xF00B988a98Ca742e7958DeF9F7823b5908715f4a"
CONTRACTS_VALIDATOR_TIMELOCK_ADDR="0xF00B988a98Ca742e7958DeF9F7823b5908715f4a"
CONTRACTS_GENESIS_TX_HASH="0xb99ebfea46cbe05a21cd80fe5597d97b204befc52a16303f579c607dc1ac2e2e"
CONTRACTS_L1_ERC20_BRIDGE_PROXY_ADDR="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
CONTRACTS_L1_ALLOW_LIST_ADDR="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
CONTRACTS_L1_ERC20_BRIDGE_IMPL_ADDR="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
CONTRACTS_L2_ERC20_BRIDGE_ADDR="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
CONTRACTS_L1_WETH_BRIDGE_PROXY_ADDR="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
CONTRACTS_L2_WETH_BRIDGE_ADDR="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
CONTRACTS_L2_TESTNET_PAYMASTER_ADDR="FC073319977e314F251EAE6ae6bE76B0B3BAeeCF"
CONTRACTS_RECURSION_SCHEDULER_LEVEL_VK_HASH="0x1186ec268d49f1905f8d9c1e9d39fc33e98c74f91d91a21b8f7ef78bd09a8db8"
CONTRACTS_RECURSION_NODE_LEVEL_VK_HASH="0x1186ec268d49f1905f8d9c1e9d39fc33e98c74f91d91a21b8f7ef78bd09a8db8"
CONTRACTS_RECURSION_LEAF_LEVEL_VK_HASH="0x101e08b00193e529145ee09823378ef51a3bc8966504064f1f6ba3f1ba863210"
CONTRACTS_RECURSION_CIRCUITS_SET_VKS_HASH="0x142a364ef2073132eaf07aa7f3d8495065be5b92a2dc14fda09b4216affed9c0"
CONTRACTS_L1_MULTICALL3_ADDR="0xcA11bde05977b3631167028862bE2a173976CA11"
CONTRACTS_FRI_RECURSION_SCHEDULER_LEVEL_VK_HASH="0x201d4c7d8e781d51a3bbd451a43a8f45240bb765b565ae6ce69192d918c3563d"
CONTRACTS_FRI_RECURSION_NODE_LEVEL_VK_HASH="0x5a3ef282b21e12fe1f4438e5bb158fc5060b160559c5158c6389d62d9fe3d080"
CONTRACTS_FRI_RECURSION_LEAF_LEVEL_VK_HASH="0x72167c43a46cf38875b267d67716edc4563861364a3c03ab7aee73498421e828"
CONTRACTS_PROVER_AT_GENESIS="fri"
CONTRACTS_SNARK_WRAPPER_VK_HASH="0x4be443afd605a782b6e56d199df2460a025c81b3dea144e135bece83612563f2"
CONTRACTS_BRIDGEHUB_PROXY_ADDR="0x35ea7f92f4c5f433efe15284e99c040110cf6297"
CONTRACTS_BRIDGEHUB_IMPL_ADDR="0x87d456da9ed212eb49d80d96afb44afddf36adf8"
CONTRACTS_STATE_TRANSITION_PROXY_ADDR="0xd90f1c081c6117241624e97cb6147257c3cb2097"
CONTRACTS_STATE_TRANSITION_IMPL_ADDR="0xc957c0e82d3bafb5ad46ffbcc66900648784eb05"
CONTRACTS_TRANSPARENT_PROXY_ADMIN_ADDR="0xdd6fa5c14e7550b4caf2aa2818d24c69cbc347e5"
CONTRACTS_GENESIS_ROOT=0x436cf80dd02a7e1a1df65be6ec9ea231ccec97c44f4c8c9cd2aa26c2feb074cd
CONTRACTS_GENESIS_BATCH_COMMITMENT=0x938016208176c5a49d47c8aa582b5d18afc4f159dfa099087770e0796948fd1a
CONTRACTS_GENESIS_ROLLUP_LEAF_INDEX=26
        "#;
        lock.set_env(config);

        let actual = ContractsConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
