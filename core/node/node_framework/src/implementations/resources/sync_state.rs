use zksync_core::sync_layer::SyncState;

use crate::resource::{Resource, ResourceId};

#[derive(Debug, Clone)]
pub struct SyncStateResource(pub SyncState);

impl Resource for SyncStateResource {
    fn resource_id() -> ResourceId {
        "sync_state".into()
    }
}
