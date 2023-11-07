//! Ethereum watcher polls the Ethereum node for PriorityQueue events.
//! New events are accepted to the zkSync network once they have the sufficient amount of L1 confirmations.
//!
//! Poll interval is configured using the `ETH_POLL_INTERVAL` constant.
//! Number of confirmations is configured using the `CONFIRMATIONS_FOR_ETH_EVENT` environment variable.

use tokio::{sync::watch, task::JoinHandle};

use std::time::Duration;

use zksync_config::ETHWatchConfig;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_eth_client::EthInterface;
use zksync_system_constants::PRIORITY_EXPIRATION;
use zksync_types::{
    ethabi::Contract, web3::types::BlockNumber as Web3BlockNumber, Address, PriorityOpId,
    ProtocolVersionId,
};

mod client;
mod event_processors;
mod metrics;
#[cfg(test)]
mod tests;

use self::{
    client::{Error, EthClient, EthHttpQueryClient, RETRY_LIMIT},
    event_processors::{
        governance_upgrades::GovernanceUpgradesEventProcessor,
        priority_ops::PriorityOpsEventProcessor, upgrades::UpgradesEventProcessor, EventProcessor,
    },
    metrics::{PollStage, METRICS},
};

#[derive(Debug)]
struct EthWatchState {
    last_seen_version_id: ProtocolVersionId,
    next_expected_priority_id: PriorityOpId,
    last_processed_ethereum_block: u64,
}

#[derive(Debug)]
pub struct EthWatch<W: EthClient + Sync> {
    client: W,
    poll_interval: Duration,
    event_processors: Vec<Box<dyn EventProcessor<W>>>,

    last_processed_ethereum_block: u64,
}

impl<W: EthClient + Sync> EthWatch<W> {
    pub async fn new(
        diamond_proxy_address: Address,
        governance_contract: Option<Contract>,
        mut client: W,
        pool: &ConnectionPool,
        poll_interval: Duration,
    ) -> Self {
        let mut storage = pool.access_storage_tagged("eth_watch").await.unwrap();

        let state = Self::initialize_state(&client, &mut storage).await;

        tracing::info!("initialized state: {:?}", state);

        let priority_ops_processor =
            PriorityOpsEventProcessor::new(state.next_expected_priority_id);
        let upgrades_processor = UpgradesEventProcessor::new(state.last_seen_version_id);
        let mut event_processors: Vec<Box<dyn EventProcessor<W>>> = vec![
            Box::new(priority_ops_processor),
            Box::new(upgrades_processor),
        ];

        if let Some(governance_contract) = governance_contract {
            let governance_upgrades_processor = GovernanceUpgradesEventProcessor::new(
                diamond_proxy_address,
                state.last_seen_version_id,
                &governance_contract,
            );
            event_processors.push(Box::new(governance_upgrades_processor))
        }

        let topics = event_processors
            .iter()
            .map(|p| p.relevant_topic())
            .collect();
        client.set_topics(topics);

        Self {
            client,
            poll_interval,
            event_processors,
            last_processed_ethereum_block: state.last_processed_ethereum_block,
        }
    }

    async fn initialize_state(client: &W, storage: &mut StorageProcessor<'_>) -> EthWatchState {
        let next_expected_priority_id: PriorityOpId = storage
            .transactions_dal()
            .last_priority_id()
            .await
            .map_or(PriorityOpId(0), |e| e + 1);

        let last_seen_version_id = storage
            .protocol_versions_dal()
            .last_version_id()
            .await
            .expect("Expected at least one (genesis) version to be present in DB");

        let last_processed_ethereum_block = match storage
            .transactions_dal()
            .get_last_processed_l1_block()
            .await
        {
            // There are some priority ops processed - start from the last processed eth block
            // but subtract 1 in case the server stopped mid-block.
            Some(block) => block.0.saturating_sub(1).into(),
            // There are no priority ops processed - to be safe, scan the last 50k blocks.
            None => client
                .finalized_block_number()
                .await
                .expect("cannot initialize eth watch: cannot get current ETH block")
                .saturating_sub(PRIORITY_EXPIRATION),
        };

        EthWatchState {
            next_expected_priority_id,
            last_seen_version_id,
            last_processed_ethereum_block,
        }
    }

    pub async fn run(
        &mut self,
        pool: ConnectionPool,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let mut timer = tokio::time::interval(self.poll_interval);
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, eth_watch is shutting down");
                break;
            }

            timer.tick().await;
            METRICS.eth_poll.inc();

            let mut storage = pool.access_storage_tagged("eth_watch").await.unwrap();
            if let Err(error) = self.loop_iteration(&mut storage).await {
                // This is an error because otherwise we could potentially miss a priority operation
                // thus entering priority mode, which is not desired.
                tracing::error!("Failed to process new blocks {}", error);
                self.last_processed_ethereum_block =
                    Self::initialize_state(&self.client, &mut storage)
                        .await
                        .last_processed_ethereum_block;
            }
        }
        Ok(())
    }

    #[tracing::instrument(skip(self, storage))]
    async fn loop_iteration(&mut self, storage: &mut StorageProcessor<'_>) -> Result<(), Error> {
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

        for processor in self.event_processors.iter_mut() {
            processor
                .process_events(storage, &self.client, events.clone())
                .await?;
        }
        self.last_processed_ethereum_block = to_block;
        Ok(())
    }
}

pub async fn start_eth_watch<E: EthInterface + Send + Sync + 'static>(
    config: ETHWatchConfig,
    pool: ConnectionPool,
    eth_gateway: E,
    diamond_proxy_addr: Address,
    governance: (Contract, Address),
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<JoinHandle<anyhow::Result<()>>> {
    let eth_client = EthHttpQueryClient::new(
        eth_gateway,
        diamond_proxy_addr,
        Some(governance.1),
        config.confirmations_for_eth_event,
    );

    let mut eth_watch = EthWatch::new(
        diamond_proxy_addr,
        Some(governance.0),
        eth_client,
        &pool,
        config.poll_interval(),
    )
    .await;

    Ok(tokio::spawn(async move {
        eth_watch.run(pool, stop_receiver).await
    }))
}
