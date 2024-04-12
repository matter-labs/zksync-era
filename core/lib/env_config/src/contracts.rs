use zksync_config::ContractsConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for ContractsConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("contracts", "CONTRACTS_")
    }
}

#[cfg(test)]
mod tests {
    use zksync_system_constants::ETHEREUM_SHARED_BRIDGE_ADDRESS;

    use super::*;
    use crate::test_utils::{addr, EnvMutex};

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> ContractsConfig {
        ContractsConfig {
            governance_addr: addr("d8dA6BF26964aF9D7eEd9e03E53415D37aA96045"),
            verifier_addr: addr("34782eE00206EAB6478F2692caa800e4A581687b"),
            default_upgrade_addr: addr("0x5e6d086f5ec079adff4fb3774cdf3e8d6a34f7e9"),
            diamond_proxy_addr: addr("F00B988a98Ca742e7958DeF9F7823b5908715f4a"),
            validator_timelock_addr: addr("F00B988a98Ca742e7958DeF9F7823b5908715f4a"),
            l1_erc20_bridge_proxy_addr: Some(addr("8656770FA78c830456B00B4fFCeE6b1De0e1b888")),
            l2_erc20_bridge_addr: Some(addr("8656770FA78c830456B00B4fFCeE6b1De0e1b888")),
            l1_weth_bridge_proxy_addr: Some(addr("8656770FA78c830456B00B4fFCeE6b1De0e1b888")),
            l2_weth_bridge_addr: Some(addr("8656770FA78c830456B00B4fFCeE6b1De0e1b888")),
            l1_shared_bridge_proxy_addr: addr("8656770FA78c830456B00B4fFCeE6b1De0e1b888"),
            l2_shared_bridge_addr: addr("8656770FA78c830456B00B4fFCeE6b1De0e1b888"),
            l2_testnet_paymaster_addr: Some(addr("FC073319977e314F251EAE6ae6bE76B0B3BAeeCF")),
            l1_multicall3_addr: addr("0xcA11bde05977b3631167028862bE2a173976CA11"),
            base_token_addr: Some(ETHEREUM_SHARED_BRIDGE_ADDRESS),
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
CONTRACTS_GOVERNANCE_ADDR="0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045"
CONTRACTS_VERIFIER_ADDR="0x34782eE00206EAB6478F2692caa800e4A581687b"
CONTRACTS_DEFAULT_UPGRADE_ADDR="0x5e6d086f5ec079adff4fb3774cdf3e8d6a34f7e9"
CONTRACTS_DIAMOND_PROXY_ADDR="0xF00B988a98Ca742e7958DeF9F7823b5908715f4a"
CONTRACTS_VALIDATOR_TIMELOCK_ADDR="0xF00B988a98Ca742e7958DeF9F7823b5908715f4a"
CONTRACTS_L1_ERC20_BRIDGE_PROXY_ADDR="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
CONTRACTS_L2_ERC20_BRIDGE_ADDR="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
CONTRACTS_L1_WETH_BRIDGE_PROXY_ADDR="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
CONTRACTS_L2_WETH_BRIDGE_ADDR="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
CONTRACTS_L2_TESTNET_PAYMASTER_ADDR="FC073319977e314F251EAE6ae6bE76B0B3BAeeCF"
CONTRACTS_L1_MULTICALL3_ADDR="0xcA11bde05977b3631167028862bE2a173976CA11"
CONTRACTS_L1_SHARED_BRIDGE_PROXY_ADDR="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
CONTRACTS_L2_SHARED_BRIDGE_ADDR="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
CONTRACTS_BASE_TOKEN_ADDR="0x0000000000000000000000000000000000000001"
        "#;
        lock.set_env(config);

        let actual = ContractsConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
