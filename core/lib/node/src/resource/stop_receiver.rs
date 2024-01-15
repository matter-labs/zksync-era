use tokio::sync::watch;

use super::Resource;

#[derive(Debug, Clone)]
pub struct StopReceiverResource(pub watch::Receiver<bool>);

impl Resource for StopReceiverResource {}

impl StopReceiverResource {
    pub const RESOURCE_NAME: &str = "common/stop_receiver";

    pub fn new(receiver: watch::Receiver<bool>) -> Self {
        Self(receiver)
    }
}
