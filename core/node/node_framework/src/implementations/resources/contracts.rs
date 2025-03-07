use zksync_config::configs::contracts::{ecosystem::L1SpecificContracts, ChainSpecificContracts};

use crate::Resource;

#[derive(Debug, Clone)]
pub struct SettlementLayerContractsResource(pub ChainSpecificContracts);

impl Resource for SettlementLayerContractsResource {
    fn name() -> String {
        "common/sl_layer_contracts".into()
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
pub struct L1ChainContractsResource(pub ChainSpecificContracts);

impl Resource for L1ChainContractsResource {
    fn name() -> String {
        "common/l1_contracts".into()
    }
}

#[derive(Debug, Clone)]
pub struct GatewayChainContractsResource(pub ChainSpecificContracts);

impl Resource for GatewayChainContractsResource {
    fn name() -> String {
        "common/gateway_chain_contracts".into()
    }
}
