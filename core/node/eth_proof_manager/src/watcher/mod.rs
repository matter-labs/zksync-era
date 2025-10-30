use std::sync::Arc;

use tokio::sync::watch;
use zksync_config::configs::eth_proof_manager::EthProofManagerConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal, DalError};
use zksync_object_store::ObjectStore;
use zksync_types::{
    ethabi::{self, Token},
    web3::BlockNumber,
    L2ChainId, H256,
};

use crate::{
    client::{EthProofManagerClient, RETRY_LIMIT},
    metrics::METRICS,
    types::FflonkFinalVerificationKey,
    watcher::events::{EventHandler, ProofRequestAcknowledgedHandler, ProofRequestProvenHandler},
};

mod events;

pub struct EthProofWatcher {
    client: Box<dyn EthProofManagerClient>,
    connection_pool: ConnectionPool<Core>,
    config: EthProofManagerConfig,
    // Chain id of the current chain, to filter events by it
    chain_id: L2ChainId,
    event_handlers: Vec<Box<dyn EventHandler>>,
}

impl EthProofWatcher {
    pub fn new(
        client: Box<dyn EthProofManagerClient>,
        connection_pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
        chain_id: L2ChainId,
        config: EthProofManagerConfig,
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
            chain_id,
            event_handlers: vec![
                Box::new(ProofRequestAcknowledgedHandler::new(
                    connection_pool.clone(),
                    chain_id,
                )),
                Box::new(ProofRequestProvenHandler::new(
                    connection_pool,
                    blob_store,
                    chain_id,
                    fflonk_vk,
                )),
            ],
        }
    }

    pub async fn run(&self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        tracing::info!("Starting eth proof watcher");

        let submitter_address: &'static str = self.client.submitter_address().to_string().leak();
        let contract_address: &'static str = self.client.contract_address().to_string().leak();

        METRICS.submitter_address[&submitter_address].set(1);
        METRICS.contract_address[&contract_address].set(1);

        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop request received, eth proof sender is shutting down");
                break;
            }

            METRICS
                .submitter_balance
                .set(self.client.submitter_balance().await?);

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
                    "Getting events from block {} to block {} for chain {}",
                    from_block,
                    to_block,
                    self.chain_id.as_u64()
                );

                let chain_id_as_topic = H256::from_slice(&ethabi::encode(&[Token::Uint(
                    self.chain_id.as_u64().into(),
                )]));

                let events = self
                    .client
                    .get_events_with_retry(
                        BlockNumber::Number(from_block.into()),
                        BlockNumber::Number(to_block.into()),
                        Some(vec![event.signature()]),
                        Some(vec![chain_id_as_topic]),
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
