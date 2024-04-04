use std::sync::Arc;

use zksync_core::sync_layer::MainNodeClient;

use crate::resource::Resource;

#[derive(Debug, Clone)]
pub struct MainNodeClientResource(pub Arc<dyn MainNodeClient>);

impl Resource for MainNodeClientResource {
    fn name() -> String {
        "external_node/main_node_client".into()
    }
}
