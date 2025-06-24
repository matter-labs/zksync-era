use smart_config::{ConfigSchema, DescribeConfig, DeserializeConfig};
use zksync_basic_types::{Address, H256};

use super::{
    ecosystem::L1SpecificContracts, EcosystemCommonContracts, SettlementLayerSpecificContracts,
};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct EcosystemContracts {
    pub bridgehub_proxy_addr: Address,
    pub state_transition_proxy_addr: Address,
    pub transparent_proxy_admin_addr: Address,
    pub l1_bytecodes_supplier_addr: Option<Address>,
    pub l1_wrapped_base_token_store: Option<Address>,
    pub server_notifier_addr: Option<Address>,
}

impl EcosystemContracts {
    fn for_tests() -> Self {
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

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct L1ContractsConfig {
    pub default_upgrade_addr: Address,
    pub diamond_proxy_addr: Address,
    pub governance_addr: Address,
    pub chain_admin_addr: Address,
    pub multicall3_addr: Address,
    pub verifier_addr: Address,
    pub validator_timelock_addr: Address,
    /// Used by the RPC API and by the node builder in wiring the BaseTokenRatioProvider layer.
    pub base_token_addr: Address,
    pub base_token_asset_id: Option<H256>,
    pub no_da_validium_l1_validator_addr: Option<Address>,
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
            base_token_addr: Address::repeat_byte(0x14),
            base_token_asset_id: Some(H256::repeat_byte(0x15)),
            chain_admin_addr: Address::repeat_byte(0x18),
            no_da_validium_l1_validator_addr: Some(Address::repeat_byte(0x1b)),
        }
    }
}

/// Contracts deployed to L2; not a config.
#[derive(Debug, Clone)]
pub struct L2Contracts {
    pub erc20_default_bridge: Address,
    pub shared_bridge_addr: Address,
    pub legacy_shared_bridge_addr: Option<Address>,
    pub timestamp_asserter_addr: Option<Address>,
    pub da_validator_addr: Option<Address>,
    pub testnet_paymaster_addr: Option<Address>,
    pub multicall3: Option<Address>,
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
    #[config(alias = "multicall3_addr")]
    pub multicall3: Option<Address>,
}

impl L2ContractsConfig {
    fn for_tests() -> Self {
        Self {
            legacy_shared_bridge_addr: Some(Address::repeat_byte(0x19)),
            testnet_paymaster_addr: Some(Address::repeat_byte(0x11)),
            timestamp_asserter_addr: Some(Address::repeat_byte(0x19)),
            da_validator_addr: Some(Address::repeat_byte(0x1a)),
            multicall3: Some(Address::repeat_byte(0x1c)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct Bridge {
    /// Address of the bridge on L1.
    #[config(alias = "proxy_addr")]
    pub l1_address: Option<Address>,
    /// Address of the bridge on L2.
    #[config(alias = "addr")]
    pub l2_address: Option<Address>,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct BridgesConfig {
    #[config(nest, alias = "shared_bridge")]
    pub shared: Bridge,
    #[config(nest, alias = "erc20_bridge")]
    pub erc20: Bridge,
    #[config(nest, alias = "weth_bridge")]
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

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct ProofManagerContracts {
    #[config(alias = "proof_manager_addr")]
    pub proof_manager_addr: Address,
    #[config(alias = "proxy_addr")]
    pub proxy_addr: Address,
    #[config(alias = "proxy_admin_addr")]
    pub proxy_admin_addr: Address,
}

impl Default for ProofManagerContracts {
    fn default() -> Self {
        Self {
            proof_manager_addr: Address::zero(),
            proxy_addr: Address::zero(),
            proxy_admin_addr: Address::zero(),
        }
    }
}

impl ProofManagerContracts {
    fn for_tests() -> Self {
        Self {
            proof_manager_addr: Address::repeat_byte(0x1d),
            proxy_addr: Address::repeat_byte(0x1e),
            proxy_admin_addr: Address::repeat_byte(0x1f),
        }
    }
}

/// Data about deployed contracts.
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct ContractsConfig {
    #[config(nest)]
    pub l1: L1ContractsConfig,
    #[config(nest)]
    pub l2: L2ContractsConfig,
    #[config(nest)]
    pub bridges: BridgesConfig,
    #[config(nest)]
    pub ecosystem_contracts: EcosystemContracts,
    // Setting default values to zero(for backwards compatibility)
    #[config(nest, default)]
    pub proof_manager_contracts: ProofManagerContracts,
}

impl ContractsConfig {
    pub fn for_tests() -> Self {
        Self {
            l1: L1ContractsConfig::for_tests(),
            l2: L2ContractsConfig::for_tests(),
            bridges: BridgesConfig::for_tests(),
            ecosystem_contracts: EcosystemContracts::for_tests(),
            proof_manager_contracts: ProofManagerContracts::for_tests(),
        }
    }

    pub fn l1_specific_contracts(&self) -> L1SpecificContracts {
        L1SpecificContracts {
            bytecodes_supplier_addr: self.ecosystem_contracts.l1_bytecodes_supplier_addr,
            wrapped_base_token_store: self.ecosystem_contracts.l1_wrapped_base_token_store,
            bridge_hub: Some(self.ecosystem_contracts.bridgehub_proxy_addr),
            shared_bridge: self.bridges.shared.l1_address,
            erc_20_bridge: self.bridges.erc20.l1_address,
            base_token_address: self.l1.base_token_addr,
            chain_admin: Some(self.l1.chain_admin_addr),
            server_notifier_addr: self.ecosystem_contracts.server_notifier_addr,
        }
    }

    pub fn l2_contracts(&self) -> L2Contracts {
        let bridge_address = self
            .bridges
            .erc20
            .l2_address
            .or(self.bridges.shared.l2_address)
            .expect("One of the l2 bridges should be presented");
        L2Contracts {
            erc20_default_bridge: bridge_address,
            shared_bridge_addr: bridge_address,
            legacy_shared_bridge_addr: self.l2.legacy_shared_bridge_addr,
            timestamp_asserter_addr: self.l2.timestamp_asserter_addr,
            da_validator_addr: self.l2.da_validator_addr,
            testnet_paymaster_addr: self.l2.testnet_paymaster_addr,
            multicall3: self.l2.multicall3,
        }
    }

    pub fn settlement_layer_specific_contracts(&self) -> SettlementLayerSpecificContracts {
        let ecosystem = &self.ecosystem_contracts;
        SettlementLayerSpecificContracts {
            ecosystem_contracts: EcosystemCommonContracts {
                bridgehub_proxy_addr: Some(ecosystem.bridgehub_proxy_addr),
                state_transition_proxy_addr: Some(ecosystem.state_transition_proxy_addr),
                multicall3: Some(self.l1.multicall3_addr),
                validator_timelock_addr: Some(self.l1.validator_timelock_addr),
            },
            chain_contracts_config: ChainContracts {
                diamond_proxy_addr: self.l1.diamond_proxy_addr,
            },
        }
    }

    pub(crate) fn insert_into_schema(schema: &mut ConfigSchema) {
        schema.insert(&Self::DESCRIPTION, "contracts").unwrap();
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
    }
}

/// Contracts specific for the chain. Should be deployed to all Settlement Layers.
#[derive(Debug, Clone)]
pub struct ChainContracts {
    pub diamond_proxy_addr: Address,
}

#[cfg(test)]
mod tests {
    use smart_config::{ConfigRepository, ConfigSchema, Environment, Yaml};

    use super::*;

    fn create_schema() -> ConfigSchema {
        let mut schema = ConfigSchema::default();
        ContractsConfig::insert_into_schema(&mut schema);
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
                chain_admin_addr: addr("0xdd6fa5c14e7550b4caf2aa2818d24c69cbc347ff"),
                base_token_addr: addr("0x0000000000000000000000000000000000000001"),
                base_token_asset_id: Some(H256::from_low_u64_be(123_456_789)),
                no_da_validium_l1_validator_addr: Some(addr(
                    "0x34782eE00206EAB6478F2692caa800e4A581687b",
                )),
            },
            l2: L2ContractsConfig {
                testnet_paymaster_addr: Some(addr("FC073319977e314F251EAE6ae6bE76B0B3BAeeCF")),
                legacy_shared_bridge_addr: Some(addr("8656770FA78c830456B00B4fFCeE6b1De0e1b888")),
                da_validator_addr: Some(addr("0xed6fa5c14e7550b4caf2aa2818d24c69cbc347ff")),
                timestamp_asserter_addr: Some(addr("0x0000000000000000000000000000000000000002")),
                multicall3: Some(addr("0x0000000000000000000000000000000000010002")),
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
            ecosystem_contracts: EcosystemContracts {
                bridgehub_proxy_addr: addr("0x35ea7f92f4c5f433efe15284e99c040110cf6297"),
                state_transition_proxy_addr: addr("0xd90f1c081c6117241624e97cb6147257c3cb2097"),
                transparent_proxy_admin_addr: addr("0xdd6fa5c14e7550b4caf2aa2818d24c69cbc347e5"),
                l1_bytecodes_supplier_addr: Some(addr("F00B988a98Ca742e7958DeF9F7823b5908715f4a")),
                l1_wrapped_base_token_store: Some(addr(
                    "0x35ea7f92f4c5f433efe15284e99c040110cf6297",
                )),
                server_notifier_addr: Some(addr("F00B988a98Ca742e7958DeF9F7823b5908715f4a")),
            },
            proof_manager_contracts: ProofManagerContracts {
                proof_manager_addr: addr("0x35ea7f92f4c5f433efe15284e99c040110cf6297"),
                proxy_addr: addr("0x35ea7f92f4c5f433efe15284e99c040110cf6297"),
                proxy_admin_addr: addr("0x35ea7f92f4c5f433efe15284e99c040110cf6297"),
            },
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

            CONTRACTS_L1_BYTECODES_SUPPLIER_ADDR="F00B988a98Ca742e7958DeF9F7823b5908715f4a"
            CONTRACTS_L1_WRAPPED_BASE_TOKEN_STORE="0x35ea7f92f4c5f433efe15284e99c040110cf6297"
            CONTRACTS_SERVER_NOTIFIER_ADDR="F00B988a98Ca742e7958DeF9F7823b5908715f4a"
            CONTRACTS_BASE_TOKEN_ASSET_ID="0x00000000000000000000000000000000000000000000000000000000075bcd15"
            CONTRACTS_NO_DA_VALIDIUM_L1_VALIDATOR_ADDR="0x34782eE00206EAB6478F2692caa800e4A581687b"
            CONTRACTS_L2_MULTICALL3_ADDR="0x0000000000000000000000000000000000010002"

            CONTRACTS_BRIDGES_SHARED_L1_ADDRESS="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
            CONTRACTS_BRIDGES_SHARED_L2_ADDRESS="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
            CONTRACTS_BRIDGES_ERC20_L1_ADDRESS="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
            CONTRACTS_BRIDGES_ERC20_L2_ADDRESS="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
            CONTRACTS_BRIDGES_WETH_L1_ADDRESS="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"
            CONTRACTS_BRIDGES_WETH_L2_ADDRESS="0x8656770FA78c830456B00B4fFCeE6b1De0e1b888"

            CONTRACTS_PROOF_MANAGER_CONTRACTS_PROOF_MANAGER_ADDR="0x35ea7f92f4c5f433efe15284e99c040110cf6297"
            CONTRACTS_PROOF_MANAGER_CONTRACTS_PROXY_ADDR="0x35ea7f92f4c5f433efe15284e99c040110cf6297"
            CONTRACTS_PROOF_MANAGER_CONTRACTS_PROXY_ADMIN_ADDR="0x35ea7f92f4c5f433efe15284e99c040110cf6297"
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
              l1_bytecodes_supplier_addr: F00B988a98Ca742e7958DeF9F7823b5908715f4a
              l1_wrapped_base_token_store: 0x35ea7f92f4c5f433efe15284e99c040110cf6297
              server_notifier_addr: F00B988a98Ca742e7958DeF9F7823b5908715f4a
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
              base_token_asset_id: '0x00000000000000000000000000000000000000000000000000000000075bcd15'
              no_da_validium_l1_validator_addr: 0x34782eE00206EAB6478F2692caa800e4A581687b
            l2:
              testnet_paymaster_addr: FC073319977e314F251EAE6ae6bE76B0B3BAeeCF
              # NOT PARSED: default_l2_upgrader: 0x512ecb6081fa5bab215aee40d3b69bcc95b565b3
              # NOT PARSED: consensus_registry: D64e136566a9E04eb05B30184fF577F52682D182
              # NOT PARSED: multicall3: 0xd72414bbdb03143ed88c6bcb126a9f77877397d8
              legacy_shared_bridge_addr: 0x8656770FA78c830456B00B4fFCeE6b1De0e1b888
              timestamp_asserter_addr: '0x0000000000000000000000000000000000000002'
              da_validator_addr: 0xed6fa5c14e7550b4caf2aa2818d24c69cbc347ff
              multicall3: '0x0000000000000000000000000000000000010002'
            proof_manager_contracts:
              proof_manager_addr: 0x35ea7f92f4c5f433efe15284e99c040110cf6297
              proxy_addr: 0x35ea7f92f4c5f433efe15284e99c040110cf6297
              proxy_admin_addr: 0x35ea7f92f4c5f433efe15284e99c040110cf6297
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let schema = create_schema();
        let repo = ConfigRepository::new(&schema).with(yaml);
        let config: ContractsConfig = repo.single().unwrap().parse().unwrap();
        assert_eq!(config, expected_config());
    }
}
