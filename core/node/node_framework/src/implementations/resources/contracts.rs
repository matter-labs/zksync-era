use zksync_config::configs::contracts::{ecosystem::L1SpecificContracts, SettlementLayerContracts};

use crate::Resource;

#[derive(Debug, Clone)]
pub struct SettlementLayerContractsResource(pub SettlementLayerContracts);

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
