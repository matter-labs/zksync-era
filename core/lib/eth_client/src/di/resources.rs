use zksync_config::configs::{
    contracts::{ecosystem::L1SpecificContracts, SettlementLayerSpecificContracts},
    eth_sender::SenderConfig,
};
use zksync_node_framework::resource::Resource;
use zksync_types::{Address, SLChainId};

use crate::BoundEthInterface;

#[derive(Debug, Clone)]
pub struct BaseL1ContractsResource(pub SettlementLayerSpecificContracts);

impl Resource for BaseL1ContractsResource {
    fn name() -> String {
        "common/base_l1_contracts".into()
    }
}

#[derive(Debug, Clone)]
pub struct BaseGatewayContractsResource {
    pub diamond_proxy_addr: Address,
    pub chain_id: SLChainId,
}

impl Resource for BaseGatewayContractsResource {
    fn name() -> String {
        "common/base_gateway_contracts".into()
    }
}

#[derive(Debug, Clone)]
pub struct BaseSettlementLayerContractsResource(pub SettlementLayerSpecificContracts);

impl Resource for BaseSettlementLayerContractsResource {
    fn name() -> String {
        "common/base_sl_layer_contracts".into()
    }
}

#[derive(Debug, Clone)]
pub struct L1EcosystemContractsResource(pub L1SpecificContracts);

impl Resource for L1EcosystemContractsResource {
    fn name() -> String {
        "common/l1_ecosystem_contracts".into()
    }
}

// FIXME: clearly out of place here. Probably remove, replacing by `SettlementModeResource` + patching config
#[derive(Debug, Clone)]
pub struct SenderConfigResource(pub SenderConfig);

impl Resource for SenderConfigResource {
    fn name() -> String {
        "common/eth_sender_config".into()
    }
}

/// A resource that provides L1 interface with signing capabilities to the service.
#[derive(Debug, Clone)]
pub struct BoundEthInterfaceResource(pub Box<dyn BoundEthInterface>);

impl Resource for BoundEthInterfaceResource {
    fn name() -> String {
        "common/bound_eth_interface".into()
    }
}

/// Same as `BoundEthInterfaceResource`, but for managing EIP-4844 blobs.
#[derive(Debug, Clone)]
pub struct BoundEthInterfaceForBlobsResource(pub Box<dyn BoundEthInterface>);

impl Resource for BoundEthInterfaceForBlobsResource {
    fn name() -> String {
        "common/bound_eth_interface_for_blobs".into()
    }
}

#[derive(Debug, Clone)]
pub struct BoundEthInterfaceForL2Resource(pub Box<dyn BoundEthInterface>);

impl Resource for BoundEthInterfaceForL2Resource {
    fn name() -> String {
        "common/bound_eth_interface_for_l2".into()
    }
}
