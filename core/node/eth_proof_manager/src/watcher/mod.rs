use std::time::Duration;

use tokio::sync::watch;
use zksync_eth_client::clients::L1;

use crate::{
    client::EthProofManagerClient,
    watcher::event_processors::{
        EventHandler, ProofRequestAcknowledgedEvent, ProofRequestProvenEvent,
    },
};

mod event_processors;

pub struct EthProofWatcher {
    client: EthProofManagerClient<L1>,
    connection_pool: ConnectionPool<Core>,
    blob_store: Arc<dyn ObjectStore>,
    poll_interval: Duration,
    event_processors: Vec<Box<dyn EventHandler>>,
}

impl EthProofWatcher {
    pub fn new(client: EthProofManagerClient<L1>, connection_pool: ConnectionPool<Core>, blob_store: Arc<dyn ObjectStore>, poll_interval: Duration) -> Self {
        Self {
            client,
            connection_pool,
            blob_store,
            poll_interval,
            event_processors: vec![
                Box::new(ProofRequestAcknowledgedEventHandler),
                Box::new(ProofRequestProvenEventHandler),
            ],
        }
    }

    pub async fn run(&self, mut stop_receiver: watch::Receiver<bool>) {
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop request received, eth proof sender is shutting down");
                break;
            }

            let to_block = self.client.get_finalized_block().await?;

            // todo: this should be changed
            let from_block = to_block.saturating_sub(100);

            for event_processor in &mut self.event_processors {
                let events = self
                    .client
                    .get_events_with_retry(
                        from_block,
                        to_block,
                        Some(vec![event_processor.signature()]),
                        None,
                        RETRY_LIMIT,
                    )
                    .await?;

                for event in events {
                    event_processor.handle_event(event, self.connection_pool.clone(), self.blob_store.clone()).await?;
                }
            }

            tokio::time::timeout(self.poll_interval, stop_receiver.changed())
                .await
                .ok();
        }
    }
}
