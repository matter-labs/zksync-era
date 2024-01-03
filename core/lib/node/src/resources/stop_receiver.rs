use tokio::sync::watch;

pub const RESOURCE_NAME: &str = "common/stop_receiver";

#[derive(Debug, Clone)]
pub struct StopReceiver(pub watch::Receiver<bool>);

impl StopReceiver {
    pub fn new(receiver: watch::Receiver<bool>) -> Self {
        Self(receiver)
    }
}
