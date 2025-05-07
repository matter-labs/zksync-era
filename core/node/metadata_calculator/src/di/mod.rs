use std::sync::Arc;

use zksync_node_framework::Resource;

pub use self::{
    calculator::MetadataCalculatorLayer, tree_api_client::TreeApiClientLayer,
    tree_api_server::TreeApiServerLayer,
};
use crate::api_server::TreeApiClient;

mod calculator;
mod tree_api_client;
mod tree_api_server;

/// A resource that provides [`TreeApiClient`] implementation to the service.
#[derive(Debug, Clone)]
pub struct TreeApiClientResource(pub Arc<dyn TreeApiClient>);

impl Resource for TreeApiClientResource {
    fn name() -> String {
        "api/tree_api_client".into()
    }
}

impl<T: TreeApiClient> From<Arc<T>> for TreeApiClientResource {
    fn from(client: Arc<T>) -> Self {
        Self(client)
    }
}
