use tokio::sync::watch;

use super::Resource;

/// Represents a receiver for the stop signal.
/// This signal is sent when the node is shutting down.
/// Every task is expected to listen to this signal and stop its execution when it is received.
#[derive(Debug, Clone)]
pub struct StopReceiverResource(pub watch::Receiver<bool>);

impl Resource for StopReceiverResource {}

impl StopReceiverResource {
    pub const RESOURCE_NAME: &str = "common/stop_receiver";

    pub fn new(receiver: watch::Receiver<bool>) -> Self {
        Self(receiver)
    }
}
