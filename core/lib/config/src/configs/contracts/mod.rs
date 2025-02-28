use zksync_basic_types::{settlement::SettlementMode, SLChainId};

use crate::configs::contracts::{
    chain::{ChainContracts, ChainContractsConfig, L2Contracts},
    ecosystem::{EcosystemCommonContracts, EcosystemL1Specific},
};

pub mod chain;
pub mod ecosystem;
pub mod gateway;

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
}

impl Contracts {
    pub fn new() -> Self {
        todo!()
    }

    pub fn gateway_chain_id(&self) -> Option<SLChainId> {
        self.gateway_chain_id
    }
}
