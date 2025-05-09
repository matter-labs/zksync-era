use zksync_node_framework::resource::{Resource, Unique};

use crate::ActionQueueSender;

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
