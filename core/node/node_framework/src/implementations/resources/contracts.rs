use zksync_config::configs::contracts::{
    chain::L2Contracts, ecosystem::L1SpecificContracts, SettlementLayerSpecificContracts,
};

use crate::Resource;

#[derive(Debug, Clone)]
pub struct SettlementLayerContractsResource(pub SettlementLayerSpecificContracts);

impl Resource for SettlementLayerContractsResource {
    fn name() -> String {
        "common/sl_layer_contracts".into()
    }
}

#[derive(Debug, Clone)]
pub struct L2ContractsResource(pub L2Contracts);

impl Resource for L2ContractsResource {
    fn name() -> String {
        "common/l2_contracts".into()
    }
}

#[derive(Debug, Clone)]
pub struct L1EcosystemContractsResource(pub L1SpecificContracts);

impl Resource for L1EcosystemContractsResource {
    fn name() -> String {
        "common/l1_ecosystem_contracts".into()
    }
}

#[derive(Debug, Clone)]
pub struct L1ChainContractsResource(pub SettlementLayerSpecificContracts);

impl Resource for L1ChainContractsResource {
    fn name() -> String {
        "common/l1_contracts".into()
    }
}

#[derive(Debug, Clone)]
pub struct GatewayChainContractsResource(pub SettlementLayerSpecificContracts);

impl Resource for GatewayChainContractsResource {
    fn name() -> String {
        "common/gateway_chain_contracts".into()
    }
}
