use zksync_node_framework::Resource;

pub use self::{
    calculator::MetadataCalculatorLayer, tree_api_client::TreeApiClientLayer,
    tree_api_server::TreeApiServerLayer,
};
use crate::api_server::TreeApiClient;

mod calculator;
mod tree_api_client;
mod tree_api_server;

impl Resource for dyn TreeApiClient {
    fn name() -> String {
        "api/tree_api_client".into()
    }
}
