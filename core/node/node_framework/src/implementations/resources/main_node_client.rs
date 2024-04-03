use std::sync::Arc;

use zksync_core::sync_layer::MainNodeClient;

use crate::resource::{Resource, ResourceId};

#[derive(Debug, Clone)]
pub struct MainNodeClientResource(pub Arc<dyn MainNodeClient>);

impl Resource for MainNodeClientResource {
    fn resource_id() -> ResourceId {
        "external_node/main_node_client".into()
    }
}
