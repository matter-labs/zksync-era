use zksync_eth_client::BoundEthInterface;
use zksync_web3_decl::client::{DynClient, L1};

use crate::resource::Resource;

/// A resource that provides L1 interface object to the service.
#[derive(Debug, Clone)]
pub struct EthInterfaceResource(pub Box<DynClient<L1>>);

impl Resource for EthInterfaceResource {
    fn name() -> String {
        "common/eth_interface".into()
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

/// A resource that provides L1 interface for Gateway.
#[derive(Debug, Clone)]
pub struct GatewayEthInterfaceResource(pub Box<DynClient<L1>>);
impl Resource for GatewayEthInterfaceResource {
    fn name() -> String {
        "common/gateway_eth_interface".into()
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
