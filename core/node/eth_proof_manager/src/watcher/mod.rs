use std::{sync::Arc, time::Duration};

use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core};
use zksync_eth_client::clients::L1;
use zksync_object_store::ObjectStore;

use crate::{
    client::{EthProofManagerClient, RETRY_LIMIT},
    watcher::events::{Event, ProofRequestAcknowledged, ProofRequestProven, RewardClaimed},
};

mod events;

pub struct EthProofWatcher {
    client: EthProofManagerClient<L1>,
    connection_pool: ConnectionPool<Core>,
    blob_store: Arc<dyn ObjectStore>,
    poll_interval: Duration,
    events: Vec<Box<dyn Event>>,
}

impl EthProofWatcher {
    pub fn new(
        client: EthProofManagerClient<L1>,
        connection_pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
        poll_interval: Duration,
    ) -> Self {
        Self {
            client,
            connection_pool,
            blob_store,
            poll_interval,
            events: vec![
                Box::new(ProofRequestAcknowledged::empty()),
                Box::new(ProofRequestProven::empty()),
                Box::new(RewardClaimed::empty()),
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

            for event in &mut self.events {
                let events = self
                    .client
                    .get_events_with_retry(
                        from_block,
                        to_block,
                        Some(vec![event.signature()]),
                        None,
                        RETRY_LIMIT,
                    )
                    .await?;

                for event in events {
                    event
                        .handle_event(event, self.connection_pool.clone(), self.blob_store.clone())
                        .await?;
                }
            }

            tokio::time::timeout(self.poll_interval, stop_receiver.changed())
                .await
                .ok();
        }
    }
}