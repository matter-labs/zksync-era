use std::sync::Arc;

use tokio::sync::watch;
use zksync_config::configs::eth_proof_manager::EthProofManagerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::web3::BlockNumber;

use crate::{
    client::{EthProofManagerClient, RETRY_LIMIT},
    watcher::events::{EventHandler, ProofRequestAcknowledgedHandler, ProofRequestProvenHandler},
};

mod events;

#[derive(Debug)]
pub struct EthProofWatcher {
    client: Box<dyn EthProofManagerClient>,
    connection_pool: ConnectionPool<Core>,
    blob_store: Arc<dyn ObjectStore>,
    config: EthProofManagerConfig,
    event_handlers: Vec<Box<dyn EventHandler>>,
}

impl EthProofWatcher {
    pub fn new(
        client: Box<dyn EthProofManagerClient>,
        connection_pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
        config: EthProofManagerConfig,
    ) -> Self {
        Self {
            client,
            connection_pool,
            blob_store,
            config,
            event_handlers: vec![
                Box::new(ProofRequestAcknowledgedHandler),
                Box::new(ProofRequestProvenHandler),
            ],
        }
    }

    pub async fn run(&self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        tracing::info!("Starting eth proof watcher");

        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop request received, eth proof sender is shutting down");
                break;
            }

            // let to_block = self.client.get_finalized_block().await?;

            // // todo: this should be changed
            // let from_block = to_block.saturating_sub(self.config.event_expiration_blocks);

            let from_block: u64 = 396773;
            let to_block: u64 = 397089;

            tracing::info!(
                "Getting events from block {} to block {}",
                from_block,
                to_block
            );

            //tracing::info!("topic: {:?}", self.event_handlers[0].signature());

            for event in &self.event_handlers {
                let events = self
                    .client
                    .get_events_with_retry(
                        BlockNumber::Number(from_block.into()),
                        BlockNumber::Number(to_block.into()),
                        Some(vec![event.signature()]),
                        None,
                        RETRY_LIMIT,
                    )
                    .await?;

                let topic = event.signature();

                tracing::info!("topic: {:?}", topic);
                tracing::info!("events: {:?}", events.len());

                for log in events {
                    event
                        .handle(log, self.connection_pool.clone(), self.blob_store.clone())
                        .await?;
                }
            }

            tokio::time::timeout(self.config.event_poll_interval, stop_receiver.changed())
                .await
                .ok();
        }

        tracing::info!("Eth proof watcher stopped");

        Ok(())
    }
}
