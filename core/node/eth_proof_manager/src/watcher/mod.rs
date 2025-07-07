use std::sync::Arc;

use tokio::sync::watch;
use zksync_config::configs::eth_proof_manager::EthProofManagerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::web3::BlockNumber;

use crate::{
    client::{EthProofManagerClient, RETRY_LIMIT},
    types::{FflonkFinalVerificationKey, PlonkFinalVerificationKey},
    watcher::events::{EventHandler, ProofRequestAcknowledgedHandler, ProofRequestProvenHandler},
};

mod events;

pub struct EthProofWatcher {
    client: Box<dyn EthProofManagerClient>,
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
        let fflonk_vk = serde_json::from_slice::<FflonkFinalVerificationKey>(
            &std::fs::read(config.path_to_fflonk_verification_key.clone()).expect(&format!(
                "Failed to read fflonk verification key at path: {}",
                config.path_to_fflonk_verification_key
            )),
        )
        .unwrap();
        let plonk_vk = serde_json::from_slice::<PlonkFinalVerificationKey>(
            &std::fs::read(config.path_to_plonk_verification_key.clone()).expect(&format!(
                "Failed to read plonk verification key at path: {}",
                config.path_to_plonk_verification_key
            )),
        )
        .unwrap();

        Self {
            client,
            config,
            event_handlers: vec![
                Box::new(ProofRequestAcknowledgedHandler::new(
                    connection_pool.clone(),
                )),
                Box::new(ProofRequestProvenHandler::new(
                    connection_pool,
                    blob_store,
                    fflonk_vk,
                    plonk_vk,
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

            let to_block = self.client.get_finalized_block().await?;

            // todo: this should be changed
            let from_block = to_block.saturating_sub(self.config.event_expiration_blocks);

            tracing::info!(
                "Getting events from block {} to block {}",
                from_block,
                to_block
            );

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
                    event.handle(log).await?;
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
