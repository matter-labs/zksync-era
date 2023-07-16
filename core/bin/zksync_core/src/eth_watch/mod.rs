//! Ethereum watcher polls the Ethereum node for PriorityQueue events.
//! New events are accepted to the zkSync network once they have the sufficient amount of L1 confirmations.
//!
//! Poll interval is configured using the `ETH_POLL_INTERVAL` constant.
//! Number of confirmations is configured using the `CONFIRMATIONS_FOR_ETH_EVENT` environment variable.

// Built-in deps
use std::time::{Duration, Instant};

// External uses
use tokio::{sync::watch, task::JoinHandle};

// Workspace deps
use zksync_config::constants::PRIORITY_EXPIRATION;
use zksync_types::{
    l1::L1Tx, web3::types::BlockNumber as Web3BlockNumber, L1BlockNumber, PriorityOpId, H160,
};

// Local deps
use self::client::{Error, EthClient};

use zksync_config::ETHWatchConfig;

use crate::eth_watch::client::{EthHttpQueryClient, RETRY_LIMIT};
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_eth_client::EthInterface;

mod client;

#[cfg(test)]
mod tests;

#[derive(Debug)]
struct EthWatchState {
    next_expected_priority_id: PriorityOpId,
    last_processed_ethereum_block: u64,
}

#[derive(Debug)]
pub struct EthWatch<W: EthClient> {
    client: W,
    poll_interval: Duration,

    state: EthWatchState,
}

impl<W: EthClient> EthWatch<W> {
    pub async fn new(client: W, pool: &ConnectionPool, poll_interval: Duration) -> Self {
        let mut storage = pool.access_storage_tagged("eth_watch").await;

        let state = Self::initialize_state(&client, &mut storage).await;

        vlog::info!("initialized state: {:?}", state);
        Self {
            client,
            poll_interval,
            state,
        }
    }

    async fn initialize_state(client: &W, storage: &mut StorageProcessor<'_>) -> EthWatchState {
        let next_expected_priority_id: PriorityOpId = storage
            .transactions_dal()
            .last_priority_id()
            .await
            .map_or(PriorityOpId(0), |e| e + 1);

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
            last_processed_ethereum_block,
        }
    }

    pub async fn run(&mut self, pool: ConnectionPool, stop_receiver: watch::Receiver<bool>) {
        let mut timer = tokio::time::interval(self.poll_interval);
        loop {
            if *stop_receiver.borrow() {
                vlog::info!("Stop signal received, eth_watch is shutting down");
                break;
            }

            timer.tick().await;

            metrics::counter!("server.eth_watch.eth_poll", 1);

            let mut storage = pool.access_storage_tagged("eth_watch").await;
            if let Err(error) = self.loop_iteration(&mut storage).await {
                // This is an error because otherwise we could potentially miss a priority operation
                // thus entering priority mode, which is not desired.
                vlog::error!("Failed to process new blocks {}", error);
                self.state = Self::initialize_state(&self.client, &mut storage).await;
            }
        }
    }

    #[tracing::instrument(skip(self, storage))]
    async fn loop_iteration(&mut self, storage: &mut StorageProcessor<'_>) -> Result<(), Error> {
        let mut stage_start = Instant::now();
        let to_block = self.client.finalized_block_number().await?;

        if to_block <= self.state.last_processed_ethereum_block {
            return Ok(());
        }

        let new_ops = self
            .get_new_priority_ops(self.state.last_processed_ethereum_block, to_block)
            .await?;

        self.state.last_processed_ethereum_block = to_block;

        metrics::histogram!("eth_watcher.poll_eth_node", stage_start.elapsed(), "stage" => "request");
        if !new_ops.is_empty() {
            let first = &new_ops[0].1;
            let last = &new_ops[new_ops.len() - 1].1;
            assert_eq!(
                first.serial_id(),
                self.state.next_expected_priority_id,
                "priority transaction serial id mismatch"
            );
            self.state.next_expected_priority_id = last.serial_id().next();
            stage_start = Instant::now();
            metrics::counter!(
                "server.processed_txs",
                new_ops.len() as u64,
                "stage" => "mempool_added"
            );
            metrics::counter!(
                "server.processed_l1_txs",
                new_ops.len() as u64,
                "stage" => "mempool_added"
            );
            for (eth_block, new_op) in new_ops {
                storage
                    .transactions_dal()
                    .insert_transaction_l1(new_op, eth_block)
                    .await;
            }
            metrics::histogram!("eth_watcher.poll_eth_node", stage_start.elapsed(), "stage" => "persist");
        }
        Ok(())
    }

    async fn get_new_priority_ops(
        &self,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<(L1BlockNumber, L1Tx)>, Error> {
        let priority_ops: Vec<L1Tx> = self
            .client
            .get_priority_op_events(
                Web3BlockNumber::Number(from_block.into()),
                Web3BlockNumber::Number(to_block.into()),
                RETRY_LIMIT,
            )
            .await?
            .into_iter()
            .collect::<Vec<_>>();

        if !priority_ops.is_empty() {
            let first = &priority_ops[0];
            let last = &priority_ops[priority_ops.len() - 1];
            vlog::debug!(
                "Received priority requests with serial ids: {} (block {}) - {} (block {})",
                first.serial_id(),
                first.eth_block(),
                last.serial_id(),
                last.eth_block(),
            );
            assert_eq!(
                last.serial_id().0 - first.serial_id().0 + 1,
                priority_ops.len() as u64,
                "there is a gap in priority ops received"
            )
        }

        Ok(priority_ops
            .into_iter()
            .skip_while(|tx| tx.serial_id() < self.state.next_expected_priority_id)
            .map(|tx| (L1BlockNumber(tx.eth_block() as u32), tx))
            .collect())
    }
}

pub async fn start_eth_watch<E: EthInterface + Send + Sync + 'static>(
    pool: ConnectionPool,
    eth_gateway: E,
    diamond_proxy_addr: H160,
    stop_receiver: watch::Receiver<bool>,
) -> JoinHandle<()> {
    let eth_watch = ETHWatchConfig::from_env();
    let eth_client = EthHttpQueryClient::new(
        eth_gateway,
        diamond_proxy_addr,
        eth_watch.confirmations_for_eth_event,
    );

    let mut eth_watch = EthWatch::new(eth_client, &pool, eth_watch.poll_interval()).await;

    tokio::spawn(async move {
        eth_watch.run(pool, stop_receiver).await;
    })
}
