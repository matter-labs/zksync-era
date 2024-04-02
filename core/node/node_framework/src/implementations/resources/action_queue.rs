use zksync_core::sync_layer::ActionQueueSender;

use crate::resource::{Resource, ResourceId, Unique};

#[derive(Debug, Clone)]
pub struct ActionQueueSenderResource(pub Unique<ActionQueueSender>);

impl Resource for ActionQueueSenderResource {
    fn resource_id() -> ResourceId {
        "external_node/action_queue_sender".into()
    }
}
