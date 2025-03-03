use zksync_basic_types::{settlement::SettlementMode, Address, SLChainId, H160};

use crate::configs::{
    contracts::{
        chain::{ChainContracts, ChainContractsConfig, L2Contracts},
        ecosystem::{EcosystemCommonContracts, EcosystemL1Specific},
    },
    gateway::{GatewayChainConfig, GatewayConfig},
};

pub mod chain;
pub mod ecosystem;
pub mod gateway;

pub const L2_BRIDGEHUB_ADDRESS: Address = H160([
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x01, 0x00, 0x02,
]);

#[derive(Debug, Clone)]
pub struct SpecificContracts {
    pub ecosystem_contracts: EcosystemCommonContracts,
    pub chain_contracts_config: ChainContracts,
    pub l2_contracts: L2Contracts,
}

#[derive(Debug, Clone)]
pub struct Contracts {
    l1_specific: EcosystemL1Specific,
    l1_contracts: SpecificContracts,
    gateway_contracts: Option<SpecificContracts>,
    sl_mode: SettlementMode,
    gateway_chain_id: Option<SLChainId>,
}

impl Contracts {
    pub fn current_contracts(&self) -> &SpecificContracts {
        match self.sl_mode {
            SettlementMode::SettlesToL1 => &self.l1_contracts,
            SettlementMode::Gateway => self.gateway_contracts.as_ref().expect("Settles to Gateway"),
        }
    }

    pub fn l1_specific_contracts(&self) -> &EcosystemL1Specific {
        &self.l1_specific
    }

    pub fn l1_contracts(&self) -> &SpecificContracts {
        &self.l1_contracts
    }

    pub fn gateway(&self) -> Option<&SpecificContracts> {
        self.gateway_contracts.as_ref()
    }
    pub fn set_settlement_mode(&mut self, settlement_mode: SettlementMode) {
        self.sl_mode = settlement_mode;
    }
}

impl Contracts {
    pub fn new(
        contracts_config: ChainContractsConfig,
        gateway_chain_config: Option<GatewayChainConfig>,
    ) -> Self {
        let ecosystem = contracts_config.ecosystem_contracts.unwrap();
        Self {
            l1_specific: EcosystemL1Specific {
                bytecodes_supplier_addr: ecosystem.l1_bytecodes_supplier_addr,
                wrapped_base_token_store: ecosystem.l1_wrapped_base_token_store,
                shared_bridge: contracts_config.l1_shared_bridge_proxy_addr,
                erc_20_bridge: contracts_config.l1_erc20_bridge_proxy_addr,
                base_token_address: contracts_config.base_token_addr,
            },
            l1_contracts: SpecificContracts {
                ecosystem_contracts: EcosystemCommonContracts {
                    bridgehub_proxy_addr: ecosystem.bridgehub_proxy_addr,
                    state_transition_proxy_addr: ecosystem.state_transition_proxy_addr,
                    server_notifier_addr: ecosystem.server_notifier_addr,
                    multicall3: contracts_config.l1_multicall3_addr,
                    validator_timelock_addr: contracts_config.validator_timelock_addr,
                },
                chain_contracts_config: ChainContracts {
                    diamond_proxy_addr: contracts_config.diamond_proxy_addr,
                    chain_admin: contracts_config.chain_admin_addr,
                },
                l2_contracts: L2Contracts {
                    l2_erc20_default_bridge: contracts_config.l2_erc20_bridge_addr,
                    l2_shared_bridge_addr: contracts_config.l2_shared_bridge_addr,
                    l2_legacy_shared_bridge_addr: contracts_config.l2_legacy_shared_bridge_addr,
                    l2_timestamp_asserter_addr: contracts_config.l2_timestamp_asserter_addr,
                    l2_da_validator_addr: contracts_config.l2_da_validator_addr,
                    l2_testnet_paymaster_addr: contracts_config.l2_testnet_paymaster_addr,
                },
            },
            gateway_chain_id: gateway_chain_config.as_ref().map(|a| a.gateway_chain_id),
            gateway_contracts: gateway_chain_config.map(|a| SpecificContracts {
                ecosystem_contracts: EcosystemCommonContracts {
                    bridgehub_proxy_addr: L2_BRIDGEHUB_ADDRESS,
                    state_transition_proxy_addr: a.state_transition_proxy_addr,
                    server_notifier_addr: a.server_notifier,
                    multicall3: a.multicall3_addr,
                    validator_timelock_addr: a.validator_timelock_addr,
                },
                chain_contracts_config: ChainContracts {
                    diamond_proxy_addr: a.diamond_proxy_addr,
                    chain_admin: a.chain_admin_addr,
                },
                l2_contracts: L2Contracts {
                    l2_erc20_default_bridge: contracts_config.l2_erc20_bridge_addr,
                    l2_shared_bridge_addr: contracts_config.l2_shared_bridge_addr,
                    l2_legacy_shared_bridge_addr: contracts_config.l2_legacy_shared_bridge_addr,
                    l2_timestamp_asserter_addr: contracts_config.l2_timestamp_asserter_addr,
                    l2_da_validator_addr: contracts_config.l2_da_validator_addr,
                    l2_testnet_paymaster_addr: contracts_config.l2_testnet_paymaster_addr,
                },
            }),
            sl_mode: Default::default(),
        }
    }

    pub fn gateway_chain_id(&self) -> Option<SLChainId> {
        self.gateway_chain_id
    }
}
