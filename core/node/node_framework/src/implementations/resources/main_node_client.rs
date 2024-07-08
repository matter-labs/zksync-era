use zksync_web3_decl::client::{DynClient, L2};

use crate::resource::Resource;

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
