use zksync_config::configs::eth_sender::SenderConfig;
use zksync_node_framework::resource::Resource;

use crate::BoundEthInterface;

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
pub struct BoundEthInterfaceForTeeDcapResource(pub Box<dyn BoundEthInterface>);

impl Resource for BoundEthInterfaceForTeeDcapResource {
    fn name() -> String {
        "common/bound_eth_interface_for_tee_dcap".into()
    }
}

#[derive(Debug, Clone)]
pub struct BoundEthInterfaceForL2Resource(pub Box<dyn BoundEthInterface>);

impl Resource for BoundEthInterfaceForL2Resource {
    fn name() -> String {
        "common/bound_eth_interface_for_l2".into()
    }
}
