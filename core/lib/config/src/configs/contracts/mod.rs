use zksync_basic_types::{settlement::SettlementMode, Address, SLChainId, H160};

use crate::configs::{
    contracts::{
        chain::{AllContractsConfig, ChainContracts},
        ecosystem::EcosystemCommonContracts,
    },
    gateway::GatewayChainConfig,
};

pub mod chain;
pub mod ecosystem;
pub mod gateway;

pub const L2_BRIDGEHUB_ADDRESS: Address = H160([
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x01, 0x00, 0x02,
]);

#[derive(Debug, Clone)]
pub struct ChainSpecificContracts {
    pub ecosystem_contracts: EcosystemCommonContracts,
    pub chain_contracts_config: ChainContracts,
}

#[derive(Debug, Clone)]
pub struct SettlementLayerContracts {
    l1_contracts: ChainSpecificContracts,
    gateway_contracts: Option<ChainSpecificContracts>,
    sl_mode: SettlementMode,
    gateway_chain_id: Option<SLChainId>,
}

impl SettlementLayerContracts {
    pub fn current_contracts(&self) -> &ChainSpecificContracts {
        match self.sl_mode {
            SettlementMode::SettlesToL1 => &self.l1_contracts,
            SettlementMode::Gateway => self.gateway_contracts.as_ref().expect("Settles to Gateway"),
        }
    }

    pub fn l1_contracts(&self) -> &ChainSpecificContracts {
        &self.l1_contracts
    }

    pub fn gateway(&self) -> Option<&ChainSpecificContracts> {
        self.gateway_contracts.as_ref()
    }
    pub fn set_settlement_mode(&mut self, settlement_mode: SettlementMode) {
        self.sl_mode = settlement_mode;
    }
    pub fn settlement_mode(&self) -> SettlementMode {
        self.sl_mode
    }
}

impl SettlementLayerContracts {
    pub fn new_raw(
        l1_contracts: ChainSpecificContracts,
        gateway_contracts: Option<ChainSpecificContracts>,
        sl_mode: SettlementMode,
    ) -> Self {
        Self {
            l1_contracts,
            gateway_contracts,
            sl_mode,
            gateway_chain_id: None,
        }
    }

    pub fn new(
        contracts_config: &AllContractsConfig,
        gateway_chain_config: Option<&GatewayChainConfig>,
    ) -> Self {
        let ecosystem = contracts_config.ecosystem_contracts.as_ref().unwrap();
        Self {
            l1_contracts: ChainSpecificContracts {
                ecosystem_contracts: EcosystemCommonContracts {
                    bridgehub_proxy_addr: Some(ecosystem.bridgehub_proxy_addr),
                    state_transition_proxy_addr: ecosystem.state_transition_proxy_addr,
                    server_notifier_addr: ecosystem.server_notifier_addr,
                    multicall3: Some(contracts_config.l1_multicall3_addr),
                    validator_timelock_addr: Some(contracts_config.validator_timelock_addr),
                    no_da_validium_l1_validator_addr: contracts_config
                        .no_da_validium_l1_validator_addr,
                },
                chain_contracts_config: ChainContracts {
                    diamond_proxy_addr: contracts_config.diamond_proxy_addr,
                    chain_admin: Some(contracts_config.chain_admin_addr),
                },
            },
            gateway_chain_id: gateway_chain_config.as_ref().map(|a| a.gateway_chain_id),
            gateway_contracts: gateway_chain_config.map(|gateway| ChainSpecificContracts {
                ecosystem_contracts: EcosystemCommonContracts {
                    bridgehub_proxy_addr: Some(L2_BRIDGEHUB_ADDRESS),
                    state_transition_proxy_addr: gateway.state_transition_proxy_addr,
                    server_notifier_addr: gateway.server_notifier,
                    multicall3: Some(gateway.multicall3_addr),
                    validator_timelock_addr: gateway.validator_timelock_addr,
                    // TODO set it properly
                    no_da_validium_l1_validator_addr: None,
                },
                chain_contracts_config: ChainContracts {
                    diamond_proxy_addr: gateway.diamond_proxy_addr,
                    chain_admin: Some(gateway.chain_admin_addr),
                },
            }),
            sl_mode: Default::default(),
        }
    }

    pub fn gateway_chain_id(&self) -> Option<SLChainId> {
        self.gateway_chain_id
    }
}
