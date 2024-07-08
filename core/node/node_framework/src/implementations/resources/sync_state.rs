use zksync_node_sync::SyncState;

use crate::resource::Resource;

/// A resource that provides [`SyncState`] to the service.
#[derive(Debug, Clone)]
pub struct SyncStateResource(pub SyncState);

impl Resource for SyncStateResource {
    fn name() -> String {
        "common/sync_state".into()
    }
}

impl From<SyncState> for SyncStateResource {
    fn from(sync_state: SyncState) -> Self {
        Self(sync_state)
    }
}
