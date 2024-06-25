use zksync_node_sync::SyncState;

use crate::resource::Resource;

#[derive(Debug, Clone)]
pub struct SyncStateResource(pub SyncState);

impl Resource for SyncStateResource {
    fn name() -> String {
        "common/sync_state".into()
    }
}
