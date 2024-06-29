//! Ethereum watcher polls the Ethereum node for the relevant events, such as priority operations (aka L1 transactions),
//! protocol upgrades etc.
//! New events are accepted to the zkSync network once they have the sufficient amount of L1 confirmations.

use std::time::Duration;

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_mini_merkle_tree::SyncMerkleTree;
use zksync_system_constants::PRIORITY_EXPIRATION;
use zksync_types::{
    ethabi::Contract, l1::L1Tx, protocol_version::ProtocolSemanticVersion,
    web3::BlockNumber as Web3BlockNumber, Address, PriorityOpId,
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

mod client;
mod event_processors;
mod metrics;
#[cfg(test)]
mod tests;

#[derive(Debug)]
struct EthWatchState {
    last_seen_protocol_version: ProtocolSemanticVersion,
    next_expected_priority_id: PriorityOpId,
    last_processed_ethereum_block: u64,
}

/// Ethereum watcher component.
#[derive(Debug)]
pub struct EthWatch {
    client: Box<dyn EthClient>,
    poll_interval: Duration,
    event_processors: Vec<Box<dyn EventProcessor>>,
    last_processed_ethereum_block: u64,
    pool: ConnectionPool<Core>,
}

impl EthWatch {
    pub async fn new(
        diamond_proxy_addr: Address,
        governance_contract: &Contract,
        mut client: Box<dyn EthClient>,
        pool: ConnectionPool<Core>,
        poll_interval: Duration,
        priority_merkle_tree: SyncMerkleTree<L1Tx>,
    ) -> anyhow::Result<Self> {
        let mut storage = pool.connection_tagged("eth_watch").await?;
        let state = Self::initialize_state(&*client, &mut storage).await?;
        tracing::info!("initialized state: {state:?}");
        drop(storage);

        let priority_ops_processor =
            PriorityOpsEventProcessor::new(state.next_expected_priority_id, priority_merkle_tree)?;
        let governance_upgrades_processor = GovernanceUpgradesEventProcessor::new(
            diamond_proxy_addr,
            state.last_seen_protocol_version,
            governance_contract,
        );
        let event_processors: Vec<Box<dyn EventProcessor>> = vec![
            Box::new(priority_ops_processor),
            Box::new(governance_upgrades_processor),
        ];

        let topics = event_processors
            .iter()
            .map(|processor| processor.relevant_topic())
            .collect();
        client.set_topics(topics);

        Ok(Self {
            client,
            poll_interval,
            event_processors,
            last_processed_ethereum_block: state.last_processed_ethereum_block,
            pool,
        })
    }

    async fn initialize_state(
        client: &dyn EthClient,
        storage: &mut Connection<'_, Core>,
    ) -> anyhow::Result<EthWatchState> {
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

        let last_processed_ethereum_block = match storage
            .transactions_dal()
            .get_last_processed_l1_block()
            .await?
        {
            // There are some priority ops processed - start from the last processed eth block
            // but subtract 1 in case the server stopped mid-block.
            Some(block) => block.0.saturating_sub(1).into(),
            // There are no priority ops processed - to be safe, scan the last 50k blocks.
            None => client
                .finalized_block_number()
                .await
                .context("cannot get current Ethereum block")?
                .saturating_sub(PRIORITY_EXPIRATION),
        };

        Ok(EthWatchState {
            next_expected_priority_id,
            last_seen_protocol_version,
            last_processed_ethereum_block,
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
                    self.last_processed_ethereum_block =
                        Self::initialize_state(&*self.client, &mut storage)
                            .await?
                            .last_processed_ethereum_block;
                }
            }
        }

        tracing::info!("Stop signal received, eth_watch is shutting down");
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn loop_iteration(
        &mut self,
        storage: &mut Connection<'_, Core>,
    ) -> Result<(), EventProcessorError> {
        let stage_latency = METRICS.poll_eth_node[&PollStage::Request].start();
        let to_block = self.client.finalized_block_number().await?;
        if to_block <= self.last_processed_ethereum_block {
            return Ok(());
        }

        let events = self
            .client
            .get_events(
                Web3BlockNumber::Number(self.last_processed_ethereum_block.into()),
                Web3BlockNumber::Number(to_block.into()),
                RETRY_LIMIT,
            )
            .await?;
        stage_latency.observe();

        for processor in &mut self.event_processors {
            let relevant_topic = processor.relevant_topic();
            let processor_events = events
                .iter()
                .filter(|event| event.topics.first() == Some(&relevant_topic))
                .cloned()
                .collect();
            processor
                .process_events(storage, &*self.client, processor_events)
                .await?;
        }
        self.last_processed_ethereum_block = to_block;
        Ok(())
    }
}
