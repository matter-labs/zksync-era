use std::sync::Arc;

use zksync_eth_client::{BoundEthInterface, EthInterface};

use crate::resource::{Resource, ResourceId};

#[derive(Debug, Clone)]
pub struct EthInterfaceResource(pub Arc<dyn EthInterface>);

impl Resource for EthInterfaceResource {
    fn resource_id() -> ResourceId {
        "common/eth_interface".into()
    }
}

#[derive(Debug, Clone)]
pub struct BoundEthInterfaceResource(pub Arc<dyn BoundEthInterface>);

impl Resource for BoundEthInterfaceResource {
    fn resource_id() -> ResourceId {
        "common/bound_eth_interface".into()
    }
}
