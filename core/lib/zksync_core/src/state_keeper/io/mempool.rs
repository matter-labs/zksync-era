use async_trait::async_trait;

use std::{
    cmp,
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use multivm::interface::{FinishedL1Batch, L1BatchEnv, SystemEnv};
use multivm::vm_latest::utils::fee::derive_base_fee_and_gas_per_pubdata;

use zksync_config::configs::chain::StateKeeperConfig;
use zksync_dal::ConnectionPool;
use zksync_mempool::L2TxFilter;
use zksync_object_store::ObjectStore;
use zksync_types::{
    block::MiniblockHeader, protocol_version::ProtocolUpgradeTx,
    witness_block_state::WitnessBlockState, Address, L1BatchNumber, L2ChainId, MiniblockNumber,
    ProtocolVersionId, Transaction, U256,
};
// TODO (SMA-1206): use seconds instead of milliseconds.
use zksync_utils::time::millis_since_epoch;

use crate::{
    l1_gas_price::L1GasPriceProvider,
    state_keeper::{
        extractors,
        io::{
            common::{l1_batch_params, load_pending_batch, poll_iters},
            MiniblockParams, MiniblockSealerHandle, PendingBatchData, StateKeeperIO,
        },
        mempool_actor::l2_tx_filter,
        metrics::KEEPER_METRICS,
        seal_criteria::{IoSealCriteria, TimeoutSealer},
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
    object_store: Box<dyn ObjectStore>,
    timeout_sealer: TimeoutSealer,
    filter: L2TxFilter,
    current_miniblock_number: MiniblockNumber,
    miniblock_sealer_handle: MiniblockSealerHandle,
    current_l1_batch_number: L1BatchNumber,
    fee_account: Address,
    fair_l2_gas_price: u64,
    validation_computational_gas_limit: u32,
    delay_interval: Duration,
    // Used to keep track of gas prices to set accepted price per pubdata byte in blocks.
    l1_gas_price_provider: Arc<G>,
    l2_erc20_bridge_addr: Address,
    chain_id: L2ChainId,

    virtual_blocks_interval: u32,
    virtual_blocks_per_miniblock: u32,
}

impl<G> IoSealCriteria for MempoolIO<G>
where
    G: L1GasPriceProvider + 'static + Send + Sync,
{
    fn should_seal_l1_batch_unconditionally(&mut self, manager: &UpdatesManager) -> bool {
        self.timeout_sealer
            .should_seal_l1_batch_unconditionally(manager)
    }

    fn should_seal_miniblock(&mut self, manager: &UpdatesManager) -> bool {
        self.timeout_sealer.should_seal_miniblock(manager)
    }
}

#[async_trait]
impl<G> StateKeeperIO for MempoolIO<G>
where
    G: L1GasPriceProvider + 'static + Send + Sync,
{
    fn current_l1_batch_number(&self) -> L1BatchNumber {
        self.current_l1_batch_number
    }

    fn current_miniblock_number(&self) -> MiniblockNumber {
        self.current_miniblock_number
    }

    async fn load_pending_batch(&mut self) -> Option<PendingBatchData> {
        let mut storage = self
            .pool
            .access_storage_tagged("state_keeper")
            .await
            .unwrap();

        let PendingBatchData {
            l1_batch_env,
            system_env,
            pending_miniblocks,
        } = load_pending_batch(
            &mut storage,
            self.current_l1_batch_number,
            self.fee_account,
            self.validation_computational_gas_limit,
            self.chain_id,
        )
        .await?;
        // Initialize the filter for the transactions that come after the pending batch.
        // We use values from the pending block to match the filter with one used before the restart.
        let (base_fee, gas_per_pubdata) = derive_base_fee_and_gas_per_pubdata(
            l1_batch_env.l1_gas_price,
            l1_batch_env.fair_l2_gas_price,
        );
        self.filter = L2TxFilter {
            l1_gas_price: l1_batch_env.l1_gas_price,
            fee_per_gas: base_fee,
            gas_per_pubdata: gas_per_pubdata as u32,
        };

        Some(PendingBatchData {
            l1_batch_env,
            system_env,
            pending_miniblocks,
        })
    }

    async fn wait_for_new_batch_params(
        &mut self,
        max_wait: Duration,
    ) -> Option<(SystemEnv, L1BatchEnv)> {
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

            let MiniblockHeader {
                timestamp: prev_miniblock_timestamp,
                hash: prev_miniblock_hash,
                ..
            } = self.load_previous_miniblock_header().await;

            // We cannot create two L1 batches or miniblocks with the same timestamp (forbidden by the bootloader).
            // Hence, we wait until the current timestamp is larger than the timestamp of the previous miniblock.
            // We can use `timeout_at` since `sleep_past` is cancel-safe; it only uses `sleep()` async calls.
            let current_timestamp = tokio::time::timeout_at(
                deadline.into(),
                sleep_past(prev_miniblock_timestamp, self.current_miniblock_number),
            );
            let current_timestamp = current_timestamp.await.ok()?;

            tracing::info!(
                "(l1_gas_price, fair_l2_gas_price) for L1 batch #{} is ({}, {})",
                self.current_l1_batch_number.0,
                self.filter.l1_gas_price,
                self.fair_l2_gas_price
            );
            let mut storage = self.pool.access_storage().await.unwrap();
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
                self.current_miniblock_number,
                prev_miniblock_hash,
                base_system_contracts,
                self.validation_computational_gas_limit,
                protocol_version,
                self.get_virtual_blocks_count(true, self.current_miniblock_number.0),
                self.chain_id,
            ));
        }
        None
    }

    // Returns the pair of timestamp and the number of virtual blocks to be produced in this miniblock
    async fn wait_for_new_miniblock_params(
        &mut self,
        max_wait: Duration,
        prev_miniblock_timestamp: u64,
    ) -> Option<MiniblockParams> {
        // We must provide different timestamps for each miniblock.
        // If miniblock sealing interval is greater than 1 second then `sleep_past` won't actually sleep.
        let timestamp = tokio::time::timeout(
            max_wait,
            sleep_past(prev_miniblock_timestamp, self.current_miniblock_number),
        )
        .await
        .ok()?;

        let virtual_blocks = self.get_virtual_blocks_count(false, self.current_miniblock_number.0);

        Some(MiniblockParams {
            timestamp,
            virtual_blocks,
        })
    }

    async fn wait_for_next_tx(&mut self, max_wait: Duration) -> Option<Transaction> {
        for _ in 0..poll_iters(self.delay_interval, max_wait) {
            let get_latency = KEEPER_METRICS.get_tx_from_mempool.start();
            let res = self.mempool.next_transaction(&self.filter);
            get_latency.observe();
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
        let mut storage = self
            .pool
            .access_storage_tagged("state_keeper")
            .await
            .unwrap();
        KEEPER_METRICS.rejected_transactions.inc();
        tracing::warn!(
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
        witness_block_state: Option<WitnessBlockState>,
        updates_manager: UpdatesManager,
        l1_batch_env: &L1BatchEnv,
        finished_batch: FinishedL1Batch,
    ) -> anyhow::Result<()> {
        assert_eq!(
            updates_manager.batch_timestamp(),
            l1_batch_env.timestamp,
            "Batch timestamps don't match, batch number {}",
            self.current_l1_batch_number()
        );

        // We cannot start sealing an L1 batch until we've sealed all miniblocks included in it.
        self.miniblock_sealer_handle.wait_for_all_commands().await;

        if let Some(witness_witness_block_state) = witness_block_state {
            match self
                .object_store
                .put(self.current_l1_batch_number(), &witness_witness_block_state)
                .await
            {
                Ok(path) => {
                    tracing::debug!("Successfully uploaded witness block start state to Object Store to path = '{path}'");
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to upload witness block start state to Object Store: {e:?}"
                    );
                }
            }
        }

        let pool = self.pool.clone();
        let mut storage = pool.access_storage_tagged("state_keeper").await.unwrap();

        updates_manager
            .seal_l1_batch(
                &mut storage,
                self.current_miniblock_number,
                l1_batch_env,
                finished_batch,
                self.l2_erc20_bridge_addr,
            )
            .await;
        self.current_miniblock_number += 1; // Due to fictive miniblock being sealed.
        self.current_l1_batch_number += 1;
        Ok(())
    }

    async fn load_previous_batch_version_id(&mut self) -> Option<ProtocolVersionId> {
        let mut storage = self.pool.access_storage().await.unwrap();
        storage
            .blocks_dal()
            .get_batch_protocol_version_id(self.current_l1_batch_number - 1)
            .await
            .unwrap()
    }

    async fn load_upgrade_tx(
        &mut self,
        version_id: ProtocolVersionId,
    ) -> Option<ProtocolUpgradeTx> {
        let mut storage = self.pool.access_storage().await.unwrap();
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
            tracing::info!(
                "Current timestamp {} for miniblock #{miniblock} is equal to previous miniblock timestamp; waiting until \
                 timestamp increases",
                extractors::display_timestamp(current_timestamp)
            );
        }
        cmp::Ordering::Greater => {
            // This situation can be triggered if the system keeper is started on a pod with a different
            // system time, or if it is buggy. Thus, a one-time error could require no actions if L1 batches
            // are expected to be generated frequently.
            tracing::error!(
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
    #[allow(clippy::too_many_arguments)]
    pub(in crate::state_keeper) async fn new(
        mempool: MempoolGuard,
        object_store: Box<dyn ObjectStore>,
        miniblock_sealer_handle: MiniblockSealerHandle,
        l1_gas_price_provider: Arc<G>,
        pool: ConnectionPool,
        config: &StateKeeperConfig,
        delay_interval: Duration,
        l2_erc20_bridge_addr: Address,
        validation_computational_gas_limit: u32,
        chain_id: L2ChainId,
    ) -> Self {
        assert!(
            config.virtual_blocks_interval > 0,
            "Virtual blocks interval must be positive"
        );
        assert!(
            config.virtual_blocks_per_miniblock > 0,
            "Virtual blocks per miniblock must be positive"
        );

        let mut storage = pool.access_storage_tagged("state_keeper").await.unwrap();
        let last_sealed_l1_batch_header = storage
            .blocks_dal()
            .get_newest_l1_batch_header()
            .await
            .unwrap();
        let last_miniblock_number = storage
            .blocks_dal()
            .get_sealed_miniblock_number()
            .await
            .unwrap();

        drop(storage);

        Self {
            mempool,
            object_store,
            pool,
            timeout_sealer: TimeoutSealer::new(config),
            filter: L2TxFilter::default(),
            // ^ Will be initialized properly on the first newly opened batch
            current_l1_batch_number: last_sealed_l1_batch_header.number + 1,
            miniblock_sealer_handle,
            current_miniblock_number: last_miniblock_number + 1,
            fee_account: config.fee_account_addr,
            fair_l2_gas_price: config.fair_l2_gas_price,
            validation_computational_gas_limit,
            delay_interval,
            l1_gas_price_provider,
            l2_erc20_bridge_addr,
            chain_id,
            virtual_blocks_interval: config.virtual_blocks_interval,
            virtual_blocks_per_miniblock: config.virtual_blocks_per_miniblock,
        }
    }

    async fn load_previous_l1_batch_hash(&self) -> U256 {
        tracing::info!(
            "Getting previous L1 batch hash for L1 batch #{}",
            self.current_l1_batch_number
        );
        let wait_latency = KEEPER_METRICS.wait_for_prev_hash_time.start();

        let mut storage = self
            .pool
            .access_storage_tagged("state_keeper")
            .await
            .unwrap();
        let (batch_hash, _) =
            extractors::wait_for_prev_l1_batch_params(&mut storage, self.current_l1_batch_number)
                .await;

        wait_latency.observe();
        tracing::info!(
            "Got previous L1 batch hash: {batch_hash:0>64x} for L1 batch #{}",
            self.current_l1_batch_number
        );
        batch_hash
    }

    async fn load_previous_miniblock_header(&self) -> MiniblockHeader {
        let load_latency = KEEPER_METRICS.load_previous_miniblock_header.start();
        let mut storage = self
            .pool
            .access_storage_tagged("state_keeper")
            .await
            .unwrap();
        let miniblock_header = storage
            .blocks_dal()
            .get_miniblock_header(self.current_miniblock_number - 1)
            .await
            .unwrap()
            .expect("Previous miniblock must be sealed and header saved to DB");
        load_latency.observe();
        miniblock_header
    }

    /// "virtual_blocks_per_miniblock" will be created either if the miniblock_number % virtual_blocks_interval == 0 or
    /// the miniblock is the first one in the batch.
    /// For instance:
    /// 1) If we want to have virtual block speed the same as the batch speed, virtual_block_interval = 10^9 and virtual_blocks_per_miniblock = 1
    /// 2) If we want to have roughly 1 virtual block per 2 miniblocks, we need to have virtual_block_interval = 2, and virtual_blocks_per_miniblock = 1
    /// 3) If we want to have 4 virtual blocks per miniblock, we need to have virtual_block_interval = 1, and virtual_blocks_per_miniblock = 4.
    fn get_virtual_blocks_count(&self, first_in_batch: bool, miniblock_number: u32) -> u32 {
        if first_in_batch || miniblock_number % self.virtual_blocks_interval == 0 {
            return self.virtual_blocks_per_miniblock;
        }
        0
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
