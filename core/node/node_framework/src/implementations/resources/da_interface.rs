use zksync_da_client::DataAvailabilityClient;

use crate::resource::Resource;

#[derive(Debug, Clone)]
pub struct EthInterfaceResource(pub Box<dyn DataAvailabilityClient<E>>);

impl Resource for EthInterfaceResource {
    fn name() -> String {
        "common/eth_interface".into()
    }
}

#[derive(Debug, Clone)]
pub struct BoundEthInterfaceResource(pub Box<dyn BoundEthInterface>);

impl Resource for BoundEthInterfaceResource {
    fn name() -> String {
        "common/bound_eth_interface".into()
    }
}

#[derive(Debug, Clone)]
pub struct BoundEthInterfaceForBlobsResource(pub Box<dyn BoundEthInterface>);

impl Resource for BoundEthInterfaceForBlobsResource {
    fn name() -> String {
        "common/bound_eth_interface_for_blobs".into()
    }
}
