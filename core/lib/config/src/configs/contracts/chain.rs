use serde::Deserialize;
use zksync_basic_types::{Address, H256};

use crate::configs::contracts::{
    ecosystem::{EcosystemCommonContracts, EcosystemContracts, L1SpecificContracts},
    ChainSpecificContracts,
};

/// Data about deployed contracts unified l1/l2 contracts and bridges. To Be Deleted
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct AllContractsConfig {
    pub governance_addr: Address,
    pub verifier_addr: Address,
    pub default_upgrade_addr: Address,
    pub diamond_proxy_addr: Address,
    pub validator_timelock_addr: Address,
    pub l1_shared_bridge_proxy_addr: Option<Address>,
    /// Contract address that serves as a shared bridge on L2.
    /// It is expected that `L2SharedBridge` is used before gateway upgrade, and `L2AssetRouter` is used after.
    pub l2_shared_bridge_addr: Option<Address>,
    /// Address of `L2SharedBridge` that was used before gateway upgrade.
    /// `None` if chain genesis used post-gateway protocol version.
    /// If present it will be used as L2 token deployer address.
    pub l2_legacy_shared_bridge_addr: Option<Address>,
    pub l1_erc20_bridge_proxy_addr: Option<Address>,
    pub l2_erc20_bridge_addr: Option<Address>,
    pub l1_weth_bridge_proxy_addr: Option<Address>,
    pub l2_weth_bridge_addr: Option<Address>,
    pub l2_testnet_paymaster_addr: Option<Address>,
    pub l2_timestamp_asserter_addr: Option<Address>,
    pub l1_multicall3_addr: Address,
    pub ecosystem_contracts: Option<EcosystemContracts>,
    // Used by the RPC API and by the node builder in wiring the BaseTokenRatioProvider layer.
    pub base_token_addr: Option<Address>,
    pub l1_base_token_asset_id: Option<H256>,

    pub chain_admin_addr: Address,
    pub l2_da_validator_addr: Option<Address>,
    pub no_da_validium_l1_validator_addr: Option<Address>,
    pub l2_multicall3_addr: Option<Address>,
}

impl AllContractsConfig {
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
            l2_legacy_shared_bridge_addr: Some(Address::repeat_byte(0x19)),
            l1_weth_bridge_proxy_addr: Some(Address::repeat_byte(0x0b)),
            l2_weth_bridge_addr: Some(Address::repeat_byte(0x0c)),
            l2_testnet_paymaster_addr: Some(Address::repeat_byte(0x11)),
            l1_multicall3_addr: Address::repeat_byte(0x12),
            l2_timestamp_asserter_addr: Some(Address::repeat_byte(0x19)),
            governance_addr: Address::repeat_byte(0x13),
            base_token_addr: Some(Address::repeat_byte(0x14)),
            l1_base_token_asset_id: Some(H256::repeat_byte(0x15)),
            ecosystem_contracts: Some(EcosystemContracts::for_tests()),
            chain_admin_addr: Address::repeat_byte(0x18),
            l2_da_validator_addr: Some(Address::repeat_byte(0x1a)),
            no_da_validium_l1_validator_addr: Some(Address::repeat_byte(0x1b)),
            l2_multicall3_addr: Some(Address::repeat_byte(0x1c)),
        }
    }
    pub fn l1_specific_contracts(&self) -> L1SpecificContracts {
        let ecosystem = self.ecosystem_contracts.as_ref().unwrap();
        L1SpecificContracts {
            bytecodes_supplier_addr: ecosystem.l1_bytecodes_supplier_addr,
            wrapped_base_token_store: ecosystem.l1_wrapped_base_token_store,
            bridge_hub: Some(ecosystem.bridgehub_proxy_addr),
            shared_bridge: self.l1_shared_bridge_proxy_addr,
            erc_20_bridge: self.l1_erc20_bridge_proxy_addr,
            base_token_address: self.base_token_addr,
        }
    }
    pub fn l2_contracts(&self) -> L2Contracts {
        L2Contracts {
            erc20_default_bridge: self.l2_erc20_bridge_addr,
            shared_bridge_addr: self.l2_shared_bridge_addr,
            legacy_shared_bridge_addr: self.l2_legacy_shared_bridge_addr,
            timestamp_asserter_addr: self.l2_timestamp_asserter_addr,
            da_validator_addr: self.l2_da_validator_addr,
            testnet_paymaster_addr: self.l2_testnet_paymaster_addr,
            multicall3: self.l2_multicall3_addr,
        }
    }

    pub fn chain_specific_contracts(&self) -> ChainSpecificContracts {
        let ecosystem = self.ecosystem_contracts.as_ref().unwrap();

        ChainSpecificContracts {
            ecosystem_contracts: EcosystemCommonContracts {
                bridgehub_proxy_addr: Some(ecosystem.bridgehub_proxy_addr),
                state_transition_proxy_addr: ecosystem.state_transition_proxy_addr,
                server_notifier_addr: ecosystem.server_notifier_addr,
                multicall3: Some(self.l1_multicall3_addr),
                validator_timelock_addr: Some(self.validator_timelock_addr),
            },
            chain_contracts_config: ChainContracts {
                diamond_proxy_addr: self.diamond_proxy_addr,
                chain_admin: Some(self.chain_admin_addr),
            },
        }
    }
}

// Contracts specific for the chain. Should be deployed to all Settlement Layers
#[derive(Debug, Clone)]
pub struct ChainContracts {
    pub diamond_proxy_addr: Address,
    pub chain_admin: Option<Address>,
}

// Contracts deployed to the l2
#[derive(Debug, Clone)]
pub struct L2Contracts {
    pub erc20_default_bridge: Option<Address>,
    pub shared_bridge_addr: Option<Address>,
    pub legacy_shared_bridge_addr: Option<Address>,
    pub timestamp_asserter_addr: Option<Address>,
    pub da_validator_addr: Option<Address>,
    pub testnet_paymaster_addr: Option<Address>,
    pub multicall3: Option<Address>,
}
