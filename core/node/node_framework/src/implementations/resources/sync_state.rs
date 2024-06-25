use zksync_node_sync::SyncState;

use crate::resource::Resource;

/// A resource that provides [`SyncState`] to the service.
#[derive(Debug, Clone)]
pub struct SyncStateResource(pub SyncState);

impl Resource for SyncStateResource {
    fn name() -> String {
        "sync_state".into()
    }
}
