use async_trait::async_trait;

use std::{
    cmp,
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use vm::{
    vm_with_bootloader::{derive_base_fee_and_gas_per_pubdata, DerivedBlockContext},
    VmBlockResult,
};
use zksync_config::configs::chain::StateKeeperConfig;
use zksync_dal::ConnectionPool;
use zksync_mempool::L2TxFilter;
use zksync_types::{
    protocol_version::ProtocolUpgradeTx, Address, L1BatchNumber, MiniblockNumber,
    ProtocolVersionId, Transaction, U256,
};
use zksync_utils::time::millis_since_epoch;

use crate::{
    l1_gas_price::L1GasPriceProvider,
    state_keeper::{
        extractors,
        io::{
            common::{l1_batch_params, load_pending_batch, poll_iters},
            L1BatchParams, MiniblockSealerHandle, PendingBatchData, StateKeeperIO,
        },
        mempool_actor::l2_tx_filter,
        updates::UpdatesManager,
        MempoolGuard,
    },
};

/// Mempool-based IO for the state keeper.
/// Receives transactions from the database through the mempool filtering logic.
/// Decides which batch parameters should be used for the new batch.
/// This is an IO for the main server application.
#[derive(Debug)]
pub(crate) struct MempoolIO<G> {
    mempool: MempoolGuard,
    pool: ConnectionPool,
    filter: L2TxFilter,
    current_miniblock_number: MiniblockNumber,
    miniblock_sealer_handle: MiniblockSealerHandle,
    current_l1_batch_number: L1BatchNumber,
    fee_account: Address,
    fair_l2_gas_price: u64,
    delay_interval: Duration,
    // Used to keep track of gas prices to set accepted price per pubdata byte in blocks.
    l1_gas_price_provider: Arc<G>,
    l2_erc20_bridge_addr: Address,
}

#[async_trait]
impl<G: L1GasPriceProvider + 'static + Send + Sync> StateKeeperIO for MempoolIO<G> {
    fn current_l1_batch_number(&self) -> L1BatchNumber {
        self.current_l1_batch_number
    }

    fn current_miniblock_number(&self) -> MiniblockNumber {
        self.current_miniblock_number
    }

    async fn load_pending_batch(&mut self) -> Option<PendingBatchData> {
        let mut storage = self.pool.access_storage_tagged("state_keeper").await;

        let PendingBatchData {
            params,
            pending_miniblocks,
        } = load_pending_batch(&mut storage, self.current_l1_batch_number, self.fee_account)
            .await?;
        // Initialize the filter for the transactions that come after the pending batch.
        // We use values from the pending block to match the filter with one used before the restart.
        let context = params.context_mode.inner_block_context().context;
        let (base_fee, gas_per_pubdata) =
            derive_base_fee_and_gas_per_pubdata(context.l1_gas_price, context.fair_l2_gas_price);
        self.filter = L2TxFilter {
            l1_gas_price: context.l1_gas_price,
            fee_per_gas: base_fee,
            gas_per_pubdata: gas_per_pubdata as u32,
        };

        Some(PendingBatchData {
            params,
            pending_miniblocks,
        })
    }

    async fn wait_for_new_batch_params(&mut self, max_wait: Duration) -> Option<L1BatchParams> {
        let deadline = Instant::now() + max_wait;

        // Block until at least one transaction in the mempool can match the filter (or timeout happens).
        // This is needed to ensure that block timestamp is not too old.
        for _ in 0..poll_iters(self.delay_interval, max_wait) {
            // We create a new filter each time, since parameters may change and a previously
            // ignored transaction in the mempool may be scheduled for the execution.
            self.filter = l2_tx_filter(self.l1_gas_price_provider.as_ref(), self.fair_l2_gas_price);
            // We only need to get the root hash when we're certain that we have a new transaction.
            if !self.mempool.has_next(&self.filter) {
                tokio::time::sleep(self.delay_interval).await;
                continue;
            }

            let prev_l1_batch_hash = self.load_previous_l1_batch_hash().await;
            let prev_miniblock_timestamp = self.load_previous_miniblock_timestamp().await;
            // We cannot create two L1 batches or miniblocks with the same timestamp (forbidden by the bootloader).
            // Hence, we wait until the current timestamp is larger than the timestamp of the previous miniblock.
            // We can use `timeout_at` since `sleep_past` is cancel-safe; it only uses `sleep()` async calls.
            let current_timestamp = tokio::time::timeout_at(
                deadline.into(),
                sleep_past(prev_miniblock_timestamp, self.current_miniblock_number),
            );
            let current_timestamp = current_timestamp.await.ok()?;

            vlog::info!(
                "(l1_gas_price, fair_l2_gas_price) for L1 batch #{} is ({}, {})",
                self.current_l1_batch_number.0,
                self.filter.l1_gas_price,
                self.fair_l2_gas_price
            );
            let mut storage = self.pool.access_storage().await;
            let (base_system_contracts, protocol_version) = storage
                .protocol_versions_dal()
                .base_system_contracts_by_timestamp(current_timestamp)
                .await;
            return Some(l1_batch_params(
                self.current_l1_batch_number,
                self.fee_account,
                current_timestamp,
                prev_l1_batch_hash,
                self.filter.l1_gas_price,
                self.fair_l2_gas_price,
                base_system_contracts,
                protocol_version,
            ));
        }
        None
    }

    async fn wait_for_new_miniblock_params(
        &mut self,
        max_wait: Duration,
        prev_miniblock_timestamp: u64,
    ) -> Option<u64> {
        // We must provide different timestamps for each miniblock.
        // If miniblock sealing interval is greater than 1 second then `sleep_past` won't actually sleep.
        let current_timestamp = tokio::time::timeout(
            max_wait,
            sleep_past(prev_miniblock_timestamp, self.current_miniblock_number),
        );
        current_timestamp.await.ok()
    }

    async fn wait_for_next_tx(&mut self, max_wait: Duration) -> Option<Transaction> {
        for _ in 0..poll_iters(self.delay_interval, max_wait) {
            let started_at = Instant::now();
            let res = self.mempool.next_transaction(&self.filter);
            metrics::histogram!(
                "server.state_keeper.get_tx_from_mempool",
                started_at.elapsed(),
            );
            if let Some(res) = res {
                return Some(res);
            } else {
                tokio::time::sleep(self.delay_interval).await;
                continue;
            }
        }
        None
    }

    async fn rollback(&mut self, tx: Transaction) {
        // Reset nonces in the mempool.
        self.mempool.rollback(&tx);
        // Insert the transaction back.
        self.mempool.insert(vec![tx], HashMap::new());
    }

    async fn reject(&mut self, rejected: &Transaction, error: &str) {
        assert!(
            !rejected.is_l1(),
            "L1 transactions should not be rejected: {}",
            error
        );

        // Reset the nonces in the mempool, but don't insert the transaction back.
        self.mempool.rollback(rejected);

        // Mark tx as rejected in the storage.
        let mut storage = self.pool.access_storage_tagged("state_keeper").await;
        metrics::increment_counter!("server.state_keeper.rejected_transactions");
        vlog::warn!(
            "transaction {} is rejected with error {}",
            rejected.hash(),
            error
        );
        storage
            .transactions_dal()
            .mark_tx_as_rejected(rejected.hash(), &format!("rejected: {}", error))
            .await;
    }

    async fn seal_miniblock(&mut self, updates_manager: &UpdatesManager) {
        let command = updates_manager.seal_miniblock_command(
            self.current_l1_batch_number,
            self.current_miniblock_number,
            self.l2_erc20_bridge_addr,
        );
        self.miniblock_sealer_handle.submit(command).await;
        self.current_miniblock_number += 1;
    }

    async fn seal_l1_batch(
        &mut self,
        block_result: VmBlockResult,
        updates_manager: UpdatesManager,
        block_context: DerivedBlockContext,
    ) {
        assert_eq!(
            updates_manager.batch_timestamp(),
            block_context.context.block_timestamp,
            "Batch timestamps don't match, batch number {}",
            self.current_l1_batch_number()
        );

        // We cannot start sealing an L1 batch until we've sealed all miniblocks included in it.
        self.miniblock_sealer_handle.wait_for_all_commands().await;

        let pool = self.pool.clone();
        let mut storage = pool.access_storage_tagged("state_keeper").await;
        updates_manager
            .seal_l1_batch(
                &mut storage,
                self.current_miniblock_number,
                self.current_l1_batch_number,
                block_result,
                block_context,
                self.l2_erc20_bridge_addr,
            )
            .await;
        self.current_miniblock_number += 1; // Due to fictive miniblock being sealed.
        self.current_l1_batch_number += 1;
    }

    async fn load_previous_batch_version_id(&mut self) -> Option<ProtocolVersionId> {
        let mut storage = self.pool.access_storage().await;
        storage
            .blocks_dal()
            .get_batch_protocol_version_id(self.current_l1_batch_number - 1)
            .await
    }

    async fn load_upgrade_tx(
        &mut self,
        version_id: ProtocolVersionId,
    ) -> Option<ProtocolUpgradeTx> {
        let mut storage = self.pool.access_storage().await;
        storage
            .protocol_versions_dal()
            .get_protocol_upgrade_tx(version_id)
            .await
    }
}

/// Sleeps until the current timestamp is larger than the provided `timestamp`.
///
/// Returns the current timestamp after the sleep. It is guaranteed to be larger than `timestamp`.
async fn sleep_past(timestamp: u64, miniblock: MiniblockNumber) -> u64 {
    let mut current_timestamp_millis = millis_since_epoch();
    let mut current_timestamp = (current_timestamp_millis / 1_000) as u64;
    match timestamp.cmp(&current_timestamp) {
        cmp::Ordering::Less => return current_timestamp,
        cmp::Ordering::Equal => {
            vlog::info!(
                "Current timestamp {} for miniblock #{miniblock} is equal to previous miniblock timestamp; waiting until \
                 timestamp increases",
                extractors::display_timestamp(current_timestamp)
            );
        }
        cmp::Ordering::Greater => {
            // This situation can be triggered if the system keeper is started on a pod with a different
            // system time, or if it is buggy. Thus, a one-time error could require no actions if L1 batches
            // are expected to be generated frequently.
            vlog::error!(
                "Previous miniblock timestamp {} is larger than the current timestamp {} for miniblock #{miniblock}",
                extractors::display_timestamp(timestamp),
                extractors::display_timestamp(current_timestamp)
            );
        }
    }

    // This loop should normally run once, since `tokio::time::sleep` sleeps *at least* the specified duration.
    // The logic is organized in a loop for marginal cases, such as the system time getting changed during `sleep()`.
    loop {
        // Time to catch up to `timestamp`; panic / underflow on subtraction is never triggered
        // since we've ensured that `timestamp >= current_timestamp`.
        let wait_seconds = timestamp - current_timestamp;
        // Time to wait until the current timestamp increases.
        let wait_millis = 1_001 - (current_timestamp_millis % 1_000) as u64;
        let wait = Duration::from_millis(wait_millis + wait_seconds * 1_000);

        tokio::time::sleep(wait).await;
        current_timestamp_millis = millis_since_epoch();
        current_timestamp = (current_timestamp_millis / 1_000) as u64;

        if current_timestamp > timestamp {
            return current_timestamp;
        }
    }
}

impl<G: L1GasPriceProvider> MempoolIO<G> {
    pub(in crate::state_keeper) async fn new(
        mempool: MempoolGuard,
        miniblock_sealer_handle: MiniblockSealerHandle,
        l1_gas_price_provider: Arc<G>,
        pool: ConnectionPool,
        config: &StateKeeperConfig,
        delay_interval: Duration,
        l2_erc20_bridge_addr: Address,
    ) -> Self {
        let mut storage = pool.access_storage_tagged("state_keeper").await;
        let last_sealed_l1_batch_header = storage.blocks_dal().get_newest_l1_batch_header().await;
        let last_miniblock_number = storage.blocks_dal().get_sealed_miniblock_number().await;
        drop(storage);

        Self {
            mempool,
            pool,
            filter: L2TxFilter::default(),
            // ^ Will be initialized properly on the first newly opened batch
            current_l1_batch_number: last_sealed_l1_batch_header.number + 1,
            miniblock_sealer_handle,
            current_miniblock_number: last_miniblock_number + 1,
            fee_account: config.fee_account_addr,
            fair_l2_gas_price: config.fair_l2_gas_price,
            delay_interval,
            l1_gas_price_provider,
            l2_erc20_bridge_addr,
        }
    }

    async fn load_previous_l1_batch_hash(&self) -> U256 {
        vlog::info!(
            "Getting previous L1 batch hash for L1 batch #{}",
            self.current_l1_batch_number
        );
        let stage_started_at: Instant = Instant::now();

        let mut storage = self.pool.access_storage_tagged("state_keeper").await;
        let (batch_hash, _) =
            extractors::wait_for_prev_l1_batch_params(&mut storage, self.current_l1_batch_number)
                .await;

        metrics::histogram!(
            "server.state_keeper.wait_for_prev_hash_time",
            stage_started_at.elapsed()
        );
        vlog::info!(
            "Got previous L1 batch hash: {batch_hash:0>64x} for L1 batch #{}",
            self.current_l1_batch_number
        );
        batch_hash
    }

    async fn load_previous_miniblock_timestamp(&self) -> u64 {
        let stage_started_at: Instant = Instant::now();

        let mut storage = self.pool.access_storage_tagged("state_keeper").await;
        let miniblock_timestamp = storage
            .blocks_dal()
            .get_miniblock_timestamp(self.current_miniblock_number - 1)
            .await
            .expect("Previous miniblock must be sealed and header saved to DB");

        metrics::histogram!(
            "server.state_keeper.get_prev_miniblock_timestamp",
            stage_started_at.elapsed()
        );
        miniblock_timestamp
    }
}

/// Getters required for testing the MempoolIO.
#[cfg(test)]
impl<G: L1GasPriceProvider> MempoolIO<G> {
    pub(super) fn filter(&self) -> &L2TxFilter {
        &self.filter
    }
}

#[cfg(test)]
mod tests {
    use tokio::time::timeout_at;

    use zksync_utils::time::seconds_since_epoch;

    use super::*;

    // This test defensively uses large deadlines in order to account for tests running in parallel etc.
    #[tokio::test]
    async fn sleeping_past_timestamp() {
        let past_timestamps = [0, 1_000, 1_000_000_000, seconds_since_epoch() - 10];
        for timestamp in past_timestamps {
            let deadline = Instant::now() + Duration::from_secs(1);
            timeout_at(deadline.into(), sleep_past(timestamp, MiniblockNumber(1)))
                .await
                .unwrap();
        }

        let current_timestamp = seconds_since_epoch();
        let deadline = Instant::now() + Duration::from_secs(2);
        let ts = timeout_at(
            deadline.into(),
            sleep_past(current_timestamp, MiniblockNumber(1)),
        )
        .await
        .unwrap();
        assert!(ts > current_timestamp);

        let future_timestamp = seconds_since_epoch() + 1;
        let deadline = Instant::now() + Duration::from_secs(3);
        let ts = timeout_at(
            deadline.into(),
            sleep_past(future_timestamp, MiniblockNumber(1)),
        )
        .await
        .unwrap();
        assert!(ts > future_timestamp);

        let future_timestamp = seconds_since_epoch() + 1;
        let deadline = Instant::now() + Duration::from_millis(100);
        // ^ This deadline is too small (we need at least 1_000ms)
        let result = timeout_at(
            deadline.into(),
            sleep_past(future_timestamp, MiniblockNumber(1)),
        )
        .await;
        assert!(result.is_err());
    }
}
