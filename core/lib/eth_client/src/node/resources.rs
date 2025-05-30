use zksync_config::configs::eth_sender::SenderConfig;
use zksync_node_framework::resource::{self, Resource};

use crate::BoundEthInterface;

// FIXME: clearly out of place here. Probably remove, replacing by `SettlementModeResource` + patching config
#[derive(Debug, Clone)]
pub struct SenderConfigResource(pub SenderConfig);

impl Resource for SenderConfigResource {
    fn name() -> String {
        "common/eth_sender_config".into()
    }
}

impl Resource<resource::Boxed> for dyn BoundEthInterface {
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
