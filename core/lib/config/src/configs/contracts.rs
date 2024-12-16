use serde::{Deserialize, Serialize};
use smart_config::{
    de::{Serde, WellKnown},
    DescribeConfig, DeserializeConfig,
};
use zksync_basic_types::Address;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, DescribeConfig, DeserializeConfig)]
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

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct L1ContractsConfig {
    pub default_upgrade_addr: Address,
    pub diamond_proxy_addr: Address,
    pub governance_addr: Address,
    pub chain_admin_addr: Option<Address>,
    pub multicall3_addr: Address,
    pub verifier_addr: Address,
    pub validator_timelock_addr: Address,
    /// Used by the RPC API and by the node builder in wiring the BaseTokenRatioProvider layer.
    pub base_token_addr: Option<Address>,
}

impl L1ContractsConfig {
    fn for_tests() -> Self {
        Self {
            verifier_addr: Address::repeat_byte(0x06),
            default_upgrade_addr: Address::repeat_byte(0x06),
            diamond_proxy_addr: Address::repeat_byte(0x09),
            validator_timelock_addr: Address::repeat_byte(0x0a),
            multicall3_addr: Address::repeat_byte(0x12),
            governance_addr: Address::repeat_byte(0x13),
            base_token_addr: Some(Address::repeat_byte(0x14)),
            chain_admin_addr: Some(Address::repeat_byte(0x18)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct L2ContractsConfig {
    pub testnet_paymaster_addr: Option<Address>,
    /// Address of `L2SharedBridge` that was used before gateway upgrade.
    /// `None` if chain genesis used post-gateway protocol version.
    /// If present it will be used as L2 token deployer address.
    pub legacy_shared_bridge_addr: Option<Address>,
    pub timestamp_asserter_addr: Option<Address>,
    pub da_validator_addr: Option<Address>,
}

impl L2ContractsConfig {
    fn for_tests() -> Self {
        Self {
            legacy_shared_bridge_addr: Some(Address::repeat_byte(0x19)),
            testnet_paymaster_addr: Some(Address::repeat_byte(0x11)),
            timestamp_asserter_addr: Some(Address::repeat_byte(0x19)),
            da_validator_addr: Some(Address::repeat_byte(0x1a)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct Bridge {
    #[serde(alias = "proxy_addr")]
    pub l1_address: Option<Address>,
    #[serde(alias = "addr")]
    pub l2_address: Option<Address>,
}

impl WellKnown for Bridge {
    type Deserializer = Serde![object];
    const DE: Self::Deserializer = Serde![object];
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct BridgesConfig {
    #[config(default, alias = "shared_bridge")]
    pub shared: Bridge,
    #[config(default, alias = "erc20_bridge")]
    pub erc20: Bridge,
    #[config(default, alias = "weth_bridge")]
    pub weth: Bridge,
}

impl BridgesConfig {
    fn for_tests() -> Self {
        Self {
            shared: Bridge {
                l1_address: Some(Address::repeat_byte(0x0e)),
                l2_address: Some(Address::repeat_byte(0x0f)),
            },
            erc20: Bridge {
                l1_address: Some(Address::repeat_byte(0x0b)),
                l2_address: Some(Address::repeat_byte(0x0c)),
            },
            weth: Bridge {
                l1_address: Some(Address::repeat_byte(0x0b)),
                l2_address: Some(Address::repeat_byte(0x0c)),
            },
        }
    }
}

/// Data about deployed contracts.
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct ContractsConfig {
    #[config(nest)] // FIXME: alias = ""?
    pub l1: L1ContractsConfig,
    #[config(nest)]
    pub l2: L2ContractsConfig,
    #[config(nest)]
    pub bridges: BridgesConfig,
    #[config(nest)]
    pub ecosystem_contracts: Option<EcosystemContracts>,
}

impl ContractsConfig {
    pub fn for_tests() -> Self {
        Self {
            l1: L1ContractsConfig::for_tests(),
            l2: L2ContractsConfig::for_tests(),
            bridges: BridgesConfig::for_tests(),
            ecosystem_contracts: Some(EcosystemContracts::for_tests()),
        }
    }
}

#[cfg(test)]
mod tests {
    use smart_config::{ConfigRepository, ConfigSchema, Environment, Yaml};

    use super::*;

    fn create_schema() -> ConfigSchema {
        let mut schema = ConfigSchema::default();
        schema
            .insert(&ContractsConfig::DESCRIPTION, "contracts")
            .unwrap();
        schema
            .single_mut(&L1ContractsConfig::DESCRIPTION)
            .unwrap()
            .push_alias("contracts")
            .unwrap();
        schema
            .single_mut(&EcosystemContracts::DESCRIPTION)
            .unwrap()
            .push_alias("contracts")
            .unwrap();
        schema
    }

    fn addr(s: &str) -> Address {
        s.parse().unwrap()
    }

    fn expected_config() -> ContractsConfig {
        ContractsConfig {
            l1: L1ContractsConfig {
                governance_addr: addr("d8dA6BF26964aF9D7eEd9e03E53415D37aA96045"),
                verifier_addr: addr("34782eE00206EAB6478F2692caa800e4A581687b"),
                default_upgrade_addr: addr("0x5e6d086f5ec079adff4fb3774cdf3e8d6a34f7e9"),
                diamond_proxy_addr: addr("F00B988a98Ca742e7958DeF9F7823b5908715f4a"),
                validator_timelock_addr: addr("F00B988a98Ca742e7958DeF9F7823b5908715f4a"),
                multicall3_addr: addr("0xcA11bde05977b3631167028862bE2a173976CA11"),
                chain_admin_addr: Some(addr("0xdd6fa5c14e7550b4caf2aa2818d24c69cbc347ff")),
                base_token_addr: Some(addr("0x0000000000000000000000000000000000000001")),
            },
            l2: L2ContractsConfig {
                testnet_paymaster_addr: Some(addr("FC073319977e314F251EAE6ae6bE76B0B3BAeeCF")),
                legacy_shared_bridge_addr: Some(addr("8656770FA78c830456B00B4fFCeE6b1De0e1b888")),
                da_validator_addr: Some(addr("0xed6fa5c14e7550b4caf2aa2818d24c69cbc347ff")),
                timestamp_asserter_addr: Some(addr("0x0000000000000000000000000000000000000002")),
            },
            bridges: BridgesConfig {
                shared: Bridge {
                    l1_address: Some(addr("8656770FA78c830456B00B4fFCeE6b1De0e1b888")),
                    l2_address: Some(addr("8656770FA78c830456B00B4fFCeE6b1De0e1b888")),
                },
                erc20: Bridge {
                    l1_address: Some(addr("8656770FA78c830456B00B4fFCeE6b1De0e1b888")),
                    l2_address: Some(addr("8656770FA78c830456B00B4fFCeE6b1De0e1b888")),
                },
                weth: Bridge {
                    l1_address: Some(addr("8656770FA78c830456B00B4fFCeE6b1De0e1b888")),
                    l2_address: Some(addr("8656770FA78c830456B00B4fFCeE6b1De0e1b888")),
                },
            },
            ecosystem_contracts: Some(EcosystemContracts {
                bridgehub_proxy_addr: addr("0x35ea7f92f4c5f433efe15284e99c040110cf6297"),
                state_transition_proxy_addr: addr("0xd90f1c081c6117241624e97cb6147257c3cb2097"),
                transparent_proxy_admin_addr: addr("0xdd6fa5c14e7550b4caf2aa2818d24c69cbc347e5"),
            }),
        }
    }

    #[test]
    fn parsing_from_env() {
        let env = r#"
            CONTRACTS_GOVERNANCE_ADDR="0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045"
            CONTRACTS_VERIFIER_ADDR="0x34782eE00206EAB6478F2692caa800e4A581687b"
            CONTRACTS_DEFAULT_UPGRADE_ADDR="0x5e6d086f5ec079adff4fb3774cdf3e8d6a34f7e9"
            CONTRACTS_DIAMOND_PROXY_ADDR="0xF00B988a98Ca742e7958DeF9F7823b5908715f4a"
            CONTRACTS_VALIDATOR_TIMELOCK_ADDR="0xF00B988a98Ca742e7958DeF9F7823b5908715f4a"
            CONTRACTS_L2_TESTNET_PAYMASTER_ADDR="FC073319977e314F251EAE6ae6bE76B0B3BAeeCF"
            CONTRACTS_L2_CONSENSUS_REGISTRY_ADDR="D64e136566a9E04eb05B30184fF577F52682D182"
            CONTRACTS_L1_MULTICALL3_ADDR="0xcA11bde05977b3631167028862bE2a173976CA11"
            CONTRACTS_L2_LEGACY_SHARED_BRIDGE_ADDR="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
            CONTRACTS_BRIDGEHUB_PROXY_ADDR="0x35ea7f92f4c5f433efe15284e99c040110cf6297"
            CONTRACTS_STATE_TRANSITION_PROXY_ADDR="0xd90f1c081c6117241624e97cb6147257c3cb2097"
            CONTRACTS_TRANSPARENT_PROXY_ADMIN_ADDR="0xdd6fa5c14e7550b4caf2aa2818d24c69cbc347e5"
            CONTRACTS_BASE_TOKEN_ADDR="0x0000000000000000000000000000000000000001"
            CONTRACTS_CHAIN_ADMIN_ADDR="0xdd6fa5c14e7550b4caf2aa2818d24c69cbc347ff"
            CONTRACTS_L2_DA_VALIDATOR_ADDR="0xed6fa5c14e7550b4caf2aa2818d24c69cbc347ff"
            CONTRACTS_L2_TIMESTAMP_ASSERTER_ADDR="0x0000000000000000000000000000000000000002"

            CONTRACTS_BRIDGES_SHARED_L1_ADDRESS="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
            CONTRACTS_BRIDGES_SHARED_L2_ADDRESS="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
            CONTRACTS_BRIDGES_ERC20_L1_ADDRESS="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
            CONTRACTS_BRIDGES_ERC20_L2_ADDRESS="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
            CONTRACTS_BRIDGES_WETH_L1_ADDRESS="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
            CONTRACTS_BRIDGES_WETH_L2_ADDRESS="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
        "#;
        let env = Environment::from_dotenv("test.env", env).unwrap();

        let schema = create_schema();
        let repo = ConfigRepository::new(&schema).with(env);
        let config: ContractsConfig = repo.single().unwrap().parse().unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          contracts:
            ecosystem_contracts:
              bridgehub_proxy_addr: 0x35ea7f92f4c5f433efe15284e99c040110cf6297
              state_transition_proxy_addr: 0xd90f1c081c6117241624e97cb6147257c3cb2097
              transparent_proxy_admin_addr: 0xdd6fa5c14e7550b4caf2aa2818d24c69cbc347e5
              # NOT PARSED: validator_timelock_addr: 0xF00B988a98Ca742e7958DeF9F7823b5908715f4a
            bridges:
              erc20:
                l1_address: 8656770FA78c830456B00B4fFCeE6b1De0e1b888
                l2_address: 8656770FA78c830456B00B4fFCeE6b1De0e1b888
              shared:
                l1_address: 8656770FA78c830456B00B4fFCeE6b1De0e1b888
                l2_address: 8656770FA78c830456B00B4fFCeE6b1De0e1b888
              weth:
                l1_address: 8656770FA78c830456B00B4fFCeE6b1De0e1b888
                l2_address: 8656770FA78c830456B00B4fFCeE6b1De0e1b888
            l1:
              default_upgrade_addr: 0x5e6d086f5ec079adff4fb3774cdf3e8d6a34f7e9
              diamond_proxy_addr: 0xF00B988a98Ca742e7958DeF9F7823b5908715f4a
              governance_addr: 0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045
              chain_admin_addr: 0xdd6fa5c14e7550b4caf2aa2818d24c69cbc347ff
              multicall3_addr: 0xcA11bde05977b3631167028862bE2a173976CA11
              verifier_addr: 0x34782eE00206EAB6478F2692caa800e4A581687b
              validator_timelock_addr: 0xF00B988a98Ca742e7958DeF9F7823b5908715f4a
              base_token_addr: '0x0000000000000000000000000000000000000001'
            l2:
              testnet_paymaster_addr: FC073319977e314F251EAE6ae6bE76B0B3BAeeCF
              # NOT PARSED: default_l2_upgrader: 0x512ecb6081fa5bab215aee40d3b69bcc95b565b3
              # NOT PARSED: consensus_registry: D64e136566a9E04eb05B30184fF577F52682D182
              # NOT PARSED: multicall3: 0xd72414bbdb03143ed88c6bcb126a9f77877397d8
              legacy_shared_bridge_addr: 0x8656770FA78c830456B00B4fFCeE6b1De0e1b888
              timestamp_asserter_addr: '0x0000000000000000000000000000000000000002'
              da_validator_addr: 0xed6fa5c14e7550b4caf2aa2818d24c69cbc347ff
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let schema = create_schema();
        let repo = ConfigRepository::new(&schema).with(yaml);
        let config: ContractsConfig = repo.single().unwrap().parse().unwrap();
        assert_eq!(config, expected_config());
    }
}
