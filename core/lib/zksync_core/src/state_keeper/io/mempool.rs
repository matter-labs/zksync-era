use std::{
    cmp,
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use async_trait::async_trait;
use multivm::{
    interface::{FinishedL1Batch, L1BatchEnv, SystemEnv},
    utils::derive_base_fee_and_gas_per_pubdata,
};
use vm_utils::storage::{l1_batch_params, L1BatchParamsProvider};
use zksync_config::configs::chain::StateKeeperConfig;
use zksync_dal::ConnectionPool;
use zksync_mempool::L2TxFilter;
use zksync_object_store::ObjectStore;
use zksync_types::{
    protocol_version::ProtocolUpgradeTx, witness_block_state::WitnessBlockState, Address,
    L1BatchNumber, L2ChainId, MiniblockNumber, ProtocolVersionId, Transaction, H256,
};
// TODO (SMA-1206): use seconds instead of milliseconds.
use zksync_utils::time::millis_since_epoch;

use crate::{
    fee_model::BatchFeeModelInputProvider,
    state_keeper::{
        extractors,
        io::{
            common::{load_pending_batch, poll_iters, IoCursor},
            fee_address_migration, MiniblockParams, MiniblockSealerHandle, PendingBatchData,
            StateKeeperIO,
        },
        mempool_actor::l2_tx_filter,
        metrics::KEEPER_METRICS,
        seal_criteria::{IoSealCriteria, TimeoutSealer},
        updates::{MiniblockUpdates, UpdatesManager},
        MempoolGuard,
    },
};

/// Mempool-based IO for the state keeper.
/// Receives transactions from the database through the mempool filtering logic.
/// Decides which batch parameters should be used for the new batch.
/// This is an IO for the main server application.
#[derive(Debug)]
pub struct MempoolIO {
    mempool: MempoolGuard,
    pool: ConnectionPool,
    object_store: Arc<dyn ObjectStore>,
    timeout_sealer: TimeoutSealer,
    filter: L2TxFilter,
    current_miniblock_number: MiniblockNumber,
    prev_miniblock_hash: H256,
    prev_miniblock_timestamp: u64,
    l1_batch_params_provider: L1BatchParamsProvider,
    miniblock_sealer_handle: MiniblockSealerHandle,
    current_l1_batch_number: L1BatchNumber,
    fee_account: Address,
    validation_computational_gas_limit: u32,
    delay_interval: Duration,
    // Used to keep track of gas prices to set accepted price per pubdata byte in blocks.
    batch_fee_input_provider: Arc<dyn BatchFeeModelInputProvider>,
    l2_shared_bridge_addr: Address,
    chain_id: L2ChainId,

    virtual_blocks_interval: u32,
    virtual_blocks_per_miniblock: u32,
}

impl IoSealCriteria for MempoolIO {
    fn should_seal_l1_batch_unconditionally(&mut self, manager: &UpdatesManager) -> bool {
        self.timeout_sealer
            .should_seal_l1_batch_unconditionally(manager)
    }

    fn should_seal_miniblock(&mut self, manager: &UpdatesManager) -> bool {
        self.timeout_sealer.should_seal_miniblock(manager)
    }
}

#[async_trait]
impl StateKeeperIO for MempoolIO {
    fn current_l1_batch_number(&self) -> L1BatchNumber {
        self.current_l1_batch_number
    }

    fn current_miniblock_number(&self) -> MiniblockNumber {
        self.current_miniblock_number
    }

    async fn load_pending_batch(&mut self) -> anyhow::Result<Option<PendingBatchData>> {
        let mut storage = self.pool.access_storage_tagged("state_keeper").await?;

        let pending_miniblock_header = self
            .l1_batch_params_provider
            .load_first_miniblock_in_batch(&mut storage, self.current_l1_batch_number)
            .await
            .with_context(|| {
                format!(
                    "failed loading first miniblock for L1 batch #{}",
                    self.current_l1_batch_number
                )
            })?;
        let Some(pending_miniblock_header) = pending_miniblock_header else {
            return Ok(None);
        };

        let (system_env, l1_batch_env) = self
            .l1_batch_params_provider
            .load_l1_batch_params(
                &mut storage,
                &pending_miniblock_header,
                self.validation_computational_gas_limit,
                self.chain_id,
            )
            .await
            .with_context(|| {
                format!(
                    "failed loading params for L1 batch #{}",
                    self.current_l1_batch_number
                )
            })?;
        let pending_batch_data = load_pending_batch(&mut storage, system_env, l1_batch_env)
            .await
            .with_context(|| {
                format!(
                    "failed loading data for re-execution for pending L1 batch #{}",
                    self.current_l1_batch_number
                )
            })?;

        let PendingBatchData {
            l1_batch_env,
            system_env,
            pending_miniblocks,
        } = pending_batch_data;
        // Initialize the filter for the transactions that come after the pending batch.
        // We use values from the pending block to match the filter with one used before the restart.
        let (base_fee, gas_per_pubdata) =
            derive_base_fee_and_gas_per_pubdata(l1_batch_env.fee_input, system_env.version.into());
        self.filter = L2TxFilter {
            fee_input: l1_batch_env.fee_input,
            fee_per_gas: base_fee,
            gas_per_pubdata: gas_per_pubdata.as_u32(),
        };

        Ok(Some(PendingBatchData {
            l1_batch_env,
            system_env,
            pending_miniblocks,
        }))
    }

    async fn wait_for_new_batch_params(
        &mut self,
        max_wait: Duration,
    ) -> anyhow::Result<Option<(SystemEnv, L1BatchEnv)>> {
        let deadline = Instant::now() + max_wait;

        // Block until at least one transaction in the mempool can match the filter (or timeout happens).
        // This is needed to ensure that block timestamp is not too old.
        for _ in 0..poll_iters(self.delay_interval, max_wait) {
            // We cannot create two L1 batches or miniblocks with the same timestamp (forbidden by the bootloader).
            // Hence, we wait until the current timestamp is larger than the timestamp of the previous miniblock.
            // We can use `timeout_at` since `sleep_past` is cancel-safe; it only uses `sleep()` async calls.
            let current_timestamp = tokio::time::timeout_at(
                deadline.into(),
                sleep_past(self.prev_miniblock_timestamp, self.current_miniblock_number),
            );
            let Some(current_timestamp) = current_timestamp.await.ok() else {
                return Ok(None);
            };

            tracing::trace!(
                "Fee input for L1 batch #{} is {:#?}",
                self.current_l1_batch_number.0,
                self.filter.fee_input
            );
            let mut storage = self.pool.access_storage_tagged("state_keeper").await?;
            let (base_system_contracts, protocol_version) = storage
                .protocol_versions_dal()
                .base_system_contracts_by_timestamp(current_timestamp)
                .await
                .context("Failed loading base system contracts")?;
            drop(storage);

            // We create a new filter each time, since parameters may change and a previously
            // ignored transaction in the mempool may be scheduled for the execution.
            self.filter = l2_tx_filter(
                self.batch_fee_input_provider.as_ref(),
                protocol_version.into(),
            )
            .await;
            if !self.mempool.has_next(&self.filter) {
                tokio::time::sleep(self.delay_interval).await;
                continue;
            }

            // We only need to get the root hash when we're certain that we have a new transaction.
            let prev_l1_batch_hash = self.wait_for_previous_l1_batch_hash().await?;
            return Ok(Some(l1_batch_params(
                self.current_l1_batch_number,
                self.fee_account,
                current_timestamp,
                prev_l1_batch_hash,
                self.filter.fee_input,
                self.current_miniblock_number,
                self.prev_miniblock_hash,
                base_system_contracts,
                self.validation_computational_gas_limit,
                protocol_version,
                self.get_virtual_blocks_count(true, self.current_miniblock_number.0),
                self.chain_id,
            )));
        }
        Ok(None)
    }

    // Returns the pair of timestamp and the number of virtual blocks to be produced in this miniblock
    async fn wait_for_new_miniblock_params(
        &mut self,
        max_wait: Duration,
    ) -> anyhow::Result<Option<MiniblockParams>> {
        // We must provide different timestamps for each miniblock.
        // If miniblock sealing interval is greater than 1 second then `sleep_past` won't actually sleep.
        let timeout_result = tokio::time::timeout(
            max_wait,
            sleep_past(self.prev_miniblock_timestamp, self.current_miniblock_number),
        )
        .await;
        let Ok(timestamp) = timeout_result else {
            return Ok(None);
        };

        let virtual_blocks = self.get_virtual_blocks_count(false, self.current_miniblock_number.0);
        Ok(Some(MiniblockParams {
            timestamp,
            virtual_blocks,
        }))
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

    async fn reject(&mut self, rejected: &Transaction, error: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            !rejected.is_l1(),
            "L1 transactions should not be rejected: {error}"
        );

        // Reset the nonces in the mempool, but don't insert the transaction back.
        self.mempool.rollback(rejected);

        // Mark tx as rejected in the storage.
        let mut storage = self.pool.access_storage_tagged("state_keeper").await?;
        KEEPER_METRICS.rejected_transactions.inc();
        tracing::warn!(
            "transaction {} is rejected with error: {error}",
            rejected.hash()
        );
        storage
            .transactions_dal()
            .mark_tx_as_rejected(rejected.hash(), &format!("rejected: {error}"))
            .await;
        Ok(())
    }

    async fn seal_miniblock(&mut self, updates_manager: &UpdatesManager) {
        let command = updates_manager.seal_miniblock_command(
            self.current_l1_batch_number,
            self.current_miniblock_number,
            self.l2_shared_bridge_addr,
            false,
        );
        self.miniblock_sealer_handle.submit(command).await;
        self.update_miniblock_fields(&updates_manager.miniblock);
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
        let mut storage = pool.access_storage_tagged("state_keeper").await?;

        let fictive_miniblock = updates_manager
            .seal_l1_batch(
                &mut storage,
                self.current_miniblock_number,
                l1_batch_env,
                finished_batch,
                self.l2_shared_bridge_addr,
            )
            .await;
        self.update_miniblock_fields(&fictive_miniblock);
        self.current_l1_batch_number += 1;
        Ok(())
    }

    async fn load_previous_batch_version_id(&mut self) -> anyhow::Result<ProtocolVersionId> {
        let mut storage = self.pool.access_storage_tagged("state_keeper").await?;
        let prev_l1_batch_number = self.current_l1_batch_number - 1;
        self.l1_batch_params_provider
            .load_l1_batch_protocol_version(&mut storage, prev_l1_batch_number)
            .await
            .with_context(|| {
                format!("failed loading protocol version for L1 batch #{prev_l1_batch_number}")
            })?
            .with_context(|| format!("L1 batch #{prev_l1_batch_number} misses protocol version"))
    }

    async fn load_upgrade_tx(
        &mut self,
        version_id: ProtocolVersionId,
    ) -> anyhow::Result<Option<ProtocolUpgradeTx>> {
        let mut storage = self.pool.access_storage_tagged("state_keeper").await?;
        Ok(storage
            .protocol_versions_dal()
            .get_protocol_upgrade_tx(version_id)
            .await)
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

impl MempoolIO {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        mempool: MempoolGuard,
        object_store: Arc<dyn ObjectStore>,
        miniblock_sealer_handle: MiniblockSealerHandle,
        batch_fee_input_provider: Arc<dyn BatchFeeModelInputProvider>,
        pool: ConnectionPool,
        config: &StateKeeperConfig,
        delay_interval: Duration,
        l2_shared_bridge_addr: Address,
        validation_computational_gas_limit: u32,
        chain_id: L2ChainId,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            config.virtual_blocks_interval > 0,
            "Virtual blocks interval must be positive"
        );
        anyhow::ensure!(
            config.virtual_blocks_per_miniblock > 0,
            "Virtual blocks per miniblock must be positive"
        );

        let mut storage = pool.access_storage_tagged("state_keeper").await?;
        let cursor = IoCursor::new(&mut storage)
            .await
            .context("failed initializing I/O cursor")?;
        let l1_batch_params_provider = L1BatchParamsProvider::new(&mut storage)
            .await
            .context("failed initializing L1 batch params provider")?;
        fee_address_migration::migrate_pending_miniblocks(&mut storage).await?;
        drop(storage);

        Ok(Self {
            mempool,
            object_store,
            pool,
            timeout_sealer: TimeoutSealer::new(config),
            filter: L2TxFilter::default(),
            // ^ Will be initialized properly on the first newly opened batch
            current_l1_batch_number: cursor.l1_batch,
            miniblock_sealer_handle,
            current_miniblock_number: cursor.next_miniblock,
            prev_miniblock_hash: cursor.prev_miniblock_hash,
            prev_miniblock_timestamp: cursor.prev_miniblock_timestamp,
            l1_batch_params_provider,
            fee_account: config.fee_account_addr,
            validation_computational_gas_limit,
            delay_interval,
            batch_fee_input_provider,
            l2_shared_bridge_addr,
            chain_id,
            virtual_blocks_interval: config.virtual_blocks_interval,
            virtual_blocks_per_miniblock: config.virtual_blocks_per_miniblock,
        })
    }

    fn update_miniblock_fields(&mut self, miniblock: &MiniblockUpdates) {
        assert_eq!(
            miniblock.number, self.current_miniblock_number.0,
            "Attempted to seal a miniblock with unexpected number"
        );
        self.current_miniblock_number += 1;
        self.prev_miniblock_hash = miniblock.get_miniblock_hash();
        self.prev_miniblock_timestamp = miniblock.timestamp;
    }

    async fn wait_for_previous_l1_batch_hash(&self) -> anyhow::Result<H256> {
        tracing::trace!(
            "Getting previous L1 batch hash for L1 batch #{}",
            self.current_l1_batch_number
        );
        let wait_latency = KEEPER_METRICS.wait_for_prev_hash_time.start();

        let mut storage = self.pool.access_storage_tagged("state_keeper").await?;
        let prev_l1_batch_number = self.current_l1_batch_number - 1;
        let (batch_hash, _) = self
            .l1_batch_params_provider
            .wait_for_l1_batch_params(&mut storage, prev_l1_batch_number)
            .await
            .with_context(|| {
                format!("error waiting for params for L1 batch #{prev_l1_batch_number}")
            })?;

        wait_latency.observe();
        tracing::trace!(
            "Got previous L1 batch hash: {batch_hash:?} for L1 batch #{}",
            self.current_l1_batch_number
        );
        Ok(batch_hash)
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
impl MempoolIO {
    pub(super) fn filter(&self) -> &L2TxFilter {
        &self.filter
    }

    pub(super) fn set_prev_miniblock_timestamp(&mut self, timestamp: u64) {
        self.prev_miniblock_timestamp = timestamp;
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
