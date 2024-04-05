use zksync_core::sync_layer::ActionQueueSender;

use crate::resource::{Resource, Unique};

#[derive(Debug, Clone)]
pub struct ActionQueueSenderResource(pub Unique<ActionQueueSender>);

impl Resource for ActionQueueSenderResource {
    fn name() -> String {
        "external_node/action_queue_sender".into()
    }
}
