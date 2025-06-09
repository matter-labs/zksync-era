use std::time::Duration;

use tokio::sync::watch;
use zksync_eth_client::clients::L1;

use crate::client::EthProofManagerClient;

mod event_processors;

pub struct EthProofWatcher {
    l1_client: EthProofManagerClient<L1>,
    poll_interval: Duration,
}

impl EthProofWatcher {
    pub fn new(l1_client: EthProofManagerClient<L1>, poll_interval: Duration) -> Self {
        Self {
            l1_client,
            poll_interval,
        }
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) {
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop request received, eth proof sender is shutting down");
                break;
            }

            tokio::time::timeout(Duration::from_secs(10), stop_receiver.changed())
                .await
                .ok();
        }
    }
}
