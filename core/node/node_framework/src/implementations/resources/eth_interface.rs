use std::sync::Arc;

use zksync_eth_client::{BoundEthInterface, EthInterface};

use crate::resource::Resource;

#[derive(Debug, Clone)]
pub struct EthInterfaceResource(pub Arc<dyn EthInterface>);

impl Resource for EthInterfaceResource {
    fn name() -> String {
        "common/eth_interface".into()
    }
}

#[derive(Debug, Clone)]
pub struct BoundEthInterfaceResource(pub Arc<dyn BoundEthInterface>);

impl Resource for BoundEthInterfaceResource {
    fn name() -> String {
        "common/bound_eth_interface".into()
    }
}
