use crate::configs::contracts::{chain::ChainContracts, ecosystem::EcosystemCommonContracts};

pub mod chain;
pub mod ecosystem;

#[derive(Debug, Clone)]
pub struct SettlementLayerSpecificContracts {
    pub ecosystem_contracts: EcosystemCommonContracts,
    pub chain_contracts_config: ChainContracts,
}
