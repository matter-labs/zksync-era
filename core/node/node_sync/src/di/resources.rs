use zksync_node_framework::resource::{Resource, Unique};

use crate::{ActionQueueSender, SyncState};

/// A resource that provides [`ActionQueueSender`] to the service.
/// This resource is unique, e.g. it's expected to be consumed by a single service.
#[derive(Debug, Clone)]
pub struct ActionQueueSenderResource(pub Unique<ActionQueueSender>);

impl Resource for ActionQueueSenderResource {
    fn name() -> String {
        "external_node/action_queue_sender".into()
    }
}

impl From<ActionQueueSender> for ActionQueueSenderResource {
    fn from(sender: ActionQueueSender) -> Self {
        Self(Unique::new(sender))
    }
}

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
