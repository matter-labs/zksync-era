//! Ethereum watcher polls the Ethereum node for the relevant events, such as priority operations (aka L1 transactions),
//! protocol upgrades etc.
//! New events are accepted to the ZKsync network once they have the sufficient amount of L1 confirmations.

use std::{
    ops::{Add, RangeInclusive, Sub},
    time::Duration,
};

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_dal::{eth_watcher_dal::EventType, Connection, ConnectionPool, Core, CoreDal, DalError};
use zksync_mini_merkle_tree::SyncMerkleTree;
use zksync_system_constants::PRIORITY_EXPIRATION;
use zksync_types::{
    ethabi::Contract, l1::L1Tx, protocol_version::ProtocolSemanticVersion,
    web3::BlockNumber as Web3BlockNumber, Address, L1ChainId, PriorityOpId, SLChainId,
};

pub use self::client::EthHttpQueryClient;
use self::{
    client::{EthClient, RETRY_LIMIT},
    event_processors::{
        EventProcessor, EventProcessorError, GovernanceUpgradesEventProcessor,
        PriorityOpsEventProcessor,
    },
    metrics::{PollStage, METRICS},
};
use crate::event_processors::{DecentralizedUpgradesEventProcessor, EventsSource};

mod client;
mod event_processors;
mod metrics;
#[cfg(test)]
mod tests;

#[derive(Debug)]
struct EthWatchState {
    last_seen_protocol_version: ProtocolSemanticVersion,
    next_expected_priority_id: PriorityOpId,
}

/// Ethereum watcher component.
#[derive(Debug)]
pub struct EthWatch {
    l1_client: Box<dyn EthClient>,
    sl_client: Box<dyn EthClient>,
    poll_interval: Duration,
    event_processors: Vec<Box<dyn EventProcessor>>,
    pool: ConnectionPool<Core>,
}

impl EthWatch {
    pub async fn new(
        diamond_proxy_addr: Address,
        governance_contract: &Contract,
        chain_admin_contract: &Contract,
        l1_client: Box<dyn EthClient>,
        sl_client: Box<dyn EthClient>,
        pool: ConnectionPool<Core>,
        poll_interval: Duration,
        priority_merkle_tree: SyncMerkleTree<L1Tx>,
    ) -> anyhow::Result<Self> {
        let mut storage = pool.connection_tagged("eth_watch").await?;
        let state = Self::initialize_state(&mut storage).await?;
        tracing::info!("initialized state: {state:?}");
        drop(storage);

        let priority_ops_processor =
            PriorityOpsEventProcessor::new(state.next_expected_priority_id, priority_merkle_tree)?;
        let governance_upgrades_processor = GovernanceUpgradesEventProcessor::new(
            diamond_proxy_addr,
            state.last_seen_protocol_version,
            governance_contract,
        );
        let decentralized_upgrades_processor = DecentralizedUpgradesEventProcessor::new(
            state.last_seen_protocol_version,
            chain_admin_contract,
        );
        let event_processors: Vec<Box<dyn EventProcessor>> = vec![
            Box::new(priority_ops_processor),
            Box::new(governance_upgrades_processor),
            Box::new(decentralized_upgrades_processor),
        ];

        Ok(Self {
            l1_client,
            sl_client,
            poll_interval,
            event_processors,
            pool,
        })
    }

    #[tracing::instrument(name = "EthWatch::initialize_state", skip_all)]
    async fn initialize_state(storage: &mut Connection<'_, Core>) -> anyhow::Result<EthWatchState> {
        let next_expected_priority_id: PriorityOpId = storage
            .transactions_dal()
            .last_priority_id()
            .await?
            .map_or(PriorityOpId(0), |e| e + 1);

        let last_seen_protocol_version = storage
            .protocol_versions_dal()
            .latest_semantic_version()
            .await?
            .context("expected at least one (genesis) version to be present in DB")?;

        Ok(EthWatchState {
            next_expected_priority_id,
            last_seen_protocol_version,
        })
    }

    pub async fn run(mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut timer = tokio::time::interval(self.poll_interval);
        let pool = self.pool.clone();

        while !*stop_receiver.borrow_and_update() {
            tokio::select! {
                _ = timer.tick() => { /* continue iterations */ }
                _ = stop_receiver.changed() => break,
            }
            METRICS.eth_poll.inc();

            let mut storage = pool.connection_tagged("eth_watch").await?;
            match self.loop_iteration(&mut storage).await {
                Ok(()) => { /* everything went fine */ }
                Err(EventProcessorError::Internal(err)) => {
                    tracing::error!("Internal error processing new blocks: {err:?}");
                    return Err(err);
                }
                Err(err) => {
                    // This is an error because otherwise we could potentially miss a priority operation
                    // thus entering priority mode, which is not desired.
                    tracing::error!("Failed to process new blocks: {err}");
                }
            }
        }

        tracing::info!("Stop signal received, eth_watch is shutting down");
        Ok(())
    }

    #[tracing::instrument(name = "EthWatch::loop_iteration", skip_all)]
    async fn loop_iteration(
        &mut self,
        storage: &mut Connection<'_, Core>,
    ) -> Result<(), EventProcessorError> {
        for processor in &mut self.event_processors {
            let client = match processor.event_source() {
                EventsSource::L1 => self.l1_client.as_ref(),
                EventsSource::SL => self.sl_client.as_ref(),
            };
            let chain_id = client.chain_id().await?;
            let finalized_block = client.finalized_block_number().await?;

            let from_block = storage
                .processed_events_dal()
                .get_or_set_next_block_to_process(
                    processor.event_type(),
                    chain_id,
                    finalized_block.saturating_sub(PRIORITY_EXPIRATION),
                )
                .await
                .map_err(DalError::generalize)?;

            let processor_events = client
                .get_events(
                    Web3BlockNumber::Number(from_block.into()),
                    Web3BlockNumber::Number(finalized_block.into()),
                    processor.relevant_topic(),
                    RETRY_LIMIT,
                )
                .await?;
            let processed_events_count = processor
                .process_events(storage, &*self.l1_client, processor_events.clone())
                .await?;

            let next_block_to_process = if processed_events_count == processor_events.len() {
                finalized_block
            } else if processed_events_count == 0 {
                //nothing was processed
                from_block
            } else {
                processor_events[processed_events_count - 1]
                    .block_number
                    .expect("Event block number is missing")
                    .try_into()
                    .unwrap()
            };

            storage
                .processed_events_dal()
                .update_next_block_to_process(
                    processor.event_type(),
                    chain_id,
                    next_block_to_process,
                )
                .await
                .map_err(DalError::generalize)?;
        }
        Ok(())
    }
}
