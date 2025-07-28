use std::sync::Arc;

use tokio::sync::watch;
use zksync_config::configs::proof_manager::ProofManagerConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal, DalError};
use zksync_object_store::ObjectStore;
use zksync_types::web3::BlockNumber;

use crate::{
    client::{BoxedProofManagerClient, RETRY_LIMIT},
    types::FflonkFinalVerificationKey,
    watcher::events::{EventHandler, ProofRequestAcknowledgedHandler, ProofRequestProvenHandler},
};

mod events;

pub struct ProofWatcher {
    client: Box<dyn BoxedProofManagerClient>,
    connection_pool: ConnectionPool<Core>,
    config: ProofManagerConfig,
    event_handlers: Vec<Box<dyn EventHandler>>,
}

impl ProofWatcher {
    pub fn new(
        client: Box<dyn BoxedProofManagerClient>,
        connection_pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
        config: ProofManagerConfig,
    ) -> Self {
        let fflonk_vk = serde_json::from_slice::<FflonkFinalVerificationKey>(
            &std::fs::read(config.path_to_fflonk_verification_key.clone()).unwrap_or_else(|_| {
                panic!(
                    "Failed to read fflonk verification key at path: {}",
                    config.path_to_fflonk_verification_key
                )
            }),
        )
        .expect("Failed to read fflonk verification key");

        Self {
            client,
            connection_pool: connection_pool.clone(),
            config,
            event_handlers: vec![
                Box::new(ProofRequestAcknowledgedHandler::new(
                    connection_pool.clone(),
                )),
                Box::new(ProofRequestProvenHandler::new(
                    connection_pool,
                    blob_store,
                    fflonk_vk,
                )),
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

            for event in &self.event_handlers {
                let to_block = self.client.get_latest_block().await?;

                let from_block = self
                    .connection_pool
                    .connection()
                    .await?
                    .eth_watcher_dal()
                    .get_or_set_next_block_to_process(
                        event.event_type(),
                        self.client.chain_id(),
                        to_block.saturating_sub(self.config.event_expiration_blocks),
                    )
                    .await
                    .map_err(DalError::generalize)?;

                tracing::info!(
                    "Getting events from block {} to block {}",
                    from_block,
                    to_block
                );

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

                for log in events.clone() {
                    event.handle(log).await?;
                }

                let next_block_to_process = if events.is_empty() {
                    //nothing was processed
                    from_block
                } else {
                    let block: u64 = events[events.len() - 1]
                        .block_number
                        .expect("Event block number is missing")
                        .try_into()
                        .unwrap();
                    block + 1
                };

                self.connection_pool
                    .connection()
                    .await?
                    .eth_watcher_dal()
                    .update_next_block_to_process(
                        event.event_type(),
                        self.client.chain_id(),
                        next_block_to_process,
                    )
                    .await
                    .map_err(DalError::generalize)?;
            }

            tokio::time::timeout(self.config.event_poll_interval, stop_receiver.changed())
                .await
                .ok();
        }

        tracing::info!("Eth proof watcher stopped");

        Ok(())
    }
}
