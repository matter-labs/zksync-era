use zksync_config::configs::contracts::ecosystem::L1SpecificContracts;
use zksync_node_framework::resource::Resource;
use zksync_types::{settlement::SettlementLayer, Address, L1ChainId, SLChainId};
use zksync_web3_decl::client::{DynClient, L1, L2};

use crate::{BoundEthInterface, EthInterface};

#[derive(Debug, Clone, Copy)]
pub struct SettlementModeResource(pub SettlementLayer);

impl Resource for SettlementModeResource {
    fn name() -> String {
        "common/settlement_mode".into()
    }
}

#[derive(Debug, Clone)]
pub struct BaseL1ContractsResource {
    pub diamond_proxy_addr: Address,
    pub state_transition_proxy_addr: Option<Address>,
    pub chain_id: L1ChainId,
}

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
pub struct BaseSettlementLayerContractsResource {
    pub diamond_proxy_addr: Address,
    pub chain_id: SLChainId,
}

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

impl From<SettlementLayerClient> for Box<dyn EthInterface> {
    fn from(value: SettlementLayerClient) -> Self {
        match value {
            SettlementLayerClient::L1(client) => Box::new(client),
            SettlementLayerClient::L2(client) => Box::new(client),
        }
    }
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
