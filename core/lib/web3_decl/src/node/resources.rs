use zksync_node_framework::resource::{self, Resource};
use zksync_types::settlement::{SettlementLayer, WorkingSettlementLayer};

use crate::client::{DynClient, L1, L2};

#[derive(Debug, Clone)]
pub struct SettlementModeResource(pub WorkingSettlementLayer);

impl Resource for SettlementModeResource {
    fn name() -> String {
        "common/settlement_mode".into()
    }
}

impl SettlementModeResource {
    pub fn new(working_settlement_layer: WorkingSettlementLayer) -> Self {
        Self(working_settlement_layer)
    }

    /// For configuring the system please use this method. It returns the settlement layer that system should assume to use
    pub fn settlement_layer(&self) -> SettlementLayer {
        self.0.settlement_layer()
    }

    /// Method returns the settlement layer, where we really should send the transactions, it could be None during the migration. Please use this method only if you know how to handle unknown settlement layer
    pub fn settlement_layer_for_sending_txs(&self) -> Option<SettlementLayer> {
        self.0.settlement_layer_for_sending_txs()
    }
}

impl Resource<resource::Boxed> for DynClient<L1> {
    fn name() -> String {
        "common/eth_interface".into()
    }
}

#[derive(Debug, Clone)]
pub enum SettlementLayerClient {
    L1(Box<DynClient<L1>>),
    Gateway(Box<DynClient<L2>>),
}

impl Resource for SettlementLayerClient {
    fn name() -> String {
        "common/settlement_layer_client".into()
    }
}

/// Provides L2 interface object to the service.
impl Resource<resource::Boxed> for DynClient<L2> {
    fn name() -> String {
        "external_node/main_node_client".into()
    }
}

#[derive(Debug, Clone)]
pub struct GatewayClientResource(pub Box<DynClient<L2>>);

impl Resource for GatewayClientResource {
    fn name() -> String {
        "common/gateway_client".into()
    }
}
