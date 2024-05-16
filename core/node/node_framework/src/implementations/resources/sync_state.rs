use zksync_core::sync_layer::SyncState;

use crate::resource::Resource;

#[derive(Debug, Clone)]
pub struct SyncStateResource(pub SyncState);

impl Resource for SyncStateResource {
    fn name() -> String {
        "sync_state".into()
    }
}
