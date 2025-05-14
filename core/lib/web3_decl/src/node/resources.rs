use zksync_node_framework::resource::Resource;
use zksync_types::settlement::{SettlementLayer, WorkingSettlementLayer};

use crate::client::{DynClient, L1, L2};

#[derive(Debug, Clone)]
pub struct SettlementModeResource(WorkingSettlementLayer);

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

/// A resource that provides L1 interface object to the service.
#[derive(Debug, Clone)]
pub struct EthInterfaceResource(pub Box<DynClient<L1>>);

impl Resource for EthInterfaceResource {
    fn name() -> String {
        "common/eth_interface".into()
    }
}

#[derive(Debug, Clone)]
pub enum SettlementLayerClient {
    L1(Box<DynClient<L1>>),
    L2(Box<DynClient<L2>>),
}

impl Resource for SettlementLayerClient {
    fn name() -> String {
        "common/settlement_layer_client".into()
    }
}

/// A resource that provides L2 interface object to the service.
/// It is expected to have the same URL as the `EthInterfaceResource`, but have different capabilities.
///
/// This resource is provided separately from `EthInterfaceResource`, to provide type safety in places, where the
/// component must work with L1-interface only and should use `EthInterfaceResource` instead.
#[derive(Debug, Clone)]
pub struct L2InterfaceResource(pub Box<DynClient<L2>>);

impl Resource for L2InterfaceResource {
    fn name() -> String {
        "common/l2_interface".into()
    }
}

/// A resource that provides L2 interface object to the service.
#[derive(Debug, Clone)]
pub struct MainNodeClientResource(pub Box<DynClient<L2>>);

impl Resource for MainNodeClientResource {
    fn name() -> String {
        "external_node/main_node_client".into()
    }
}

impl<T: Into<Box<DynClient<L2>>>> From<T> for MainNodeClientResource {
    fn from(client: T) -> Self {
        Self(client.into())
    }
}
