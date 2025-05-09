use tokio::sync::watch;

/// Represents a receiver for the stop request.
/// This signal is sent when the node is shutting down.
/// Every task is expected to listen to this signal and stop its execution when it is received.
///
/// This structure exists as a first-class entity instead of being a resource to make it more visible
/// and prevent tasks from hanging by accident.
#[derive(Debug, Clone)]
pub struct StopReceiver(pub watch::Receiver<bool>);
