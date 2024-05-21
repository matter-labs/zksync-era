use std::{
    cmp,
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use async_trait::async_trait;
use multivm::{interface::Halt, utils::derive_base_fee_and_gas_per_pubdata};
use vm_utils::storage::L1BatchParamsProvider;
use zksync_config::configs::chain::StateKeeperConfig;
use zksync_contracts::BaseSystemContracts;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_mempool::L2TxFilter;
use zksync_node_fee_model::BatchFeeModelInputProvider;
use zksync_types::{
    protocol_upgrade::ProtocolUpgradeTx, utils::display_timestamp, Address, L1BatchNumber,
    L2BlockNumber, L2ChainId, ProtocolVersionId, Transaction, H256, U256,
};
// TODO (SMA-1206): use seconds instead of milliseconds.
use zksync_utils::time::millis_since_epoch;

use crate::{
    io::{
        common::{load_pending_batch, poll_iters, IoCursor},
        seal_logic::l2_block_seal_subtasks::L2BlockSealProcess,
        L1BatchParams, L2BlockParams, PendingBatchData, StateKeeperIO,
    },
    mempool_actor::l2_tx_filter,
    metrics::KEEPER_METRICS,
    seal_criteria::{IoSealCriteria, L2BlockMaxPayloadSizeSealer, TimeoutSealer},
    updates::UpdatesManager,
    MempoolGuard,
};

/// Mempool-based sequencer for the state keeper.
/// Receives transactions from the database through the mempool filtering logic.
/// Decides which batch parameters should be used for the new batch.
/// This is an IO for the main server application.
#[derive(Debug)]
pub struct MempoolIO {
    mempool: MempoolGuard,
    pool: ConnectionPool<Core>,
    timeout_sealer: TimeoutSealer,
    l2_block_max_payload_size_sealer: L2BlockMaxPayloadSizeSealer,
    filter: L2TxFilter,
    l1_batch_params_provider: L1BatchParamsProvider,
    fee_account: Address,
    validation_computational_gas_limit: u32,
    max_allowed_tx_gas_limit: U256,
    delay_interval: Duration,
    // Used to keep track of gas prices to set accepted price per pubdata byte in blocks.
    batch_fee_input_provider: Arc<dyn BatchFeeModelInputProvider>,
    chain_id: L2ChainId,
}

impl IoSealCriteria for MempoolIO {
    fn should_seal_l1_batch_unconditionally(&mut self, manager: &UpdatesManager) -> bool {
        self.timeout_sealer
            .should_seal_l1_batch_unconditionally(manager)
    }

    fn should_seal_l2_block(&mut self, manager: &UpdatesManager) -> bool {
        if self.timeout_sealer.should_seal_l2_block(manager) {
            return true;
        }
        self.l2_block_max_payload_size_sealer
            .should_seal_l2_block(manager)
    }
}

#[async_trait]
impl StateKeeperIO for MempoolIO {
    fn chain_id(&self) -> L2ChainId {
        self.chain_id
    }

    async fn initialize(&mut self) -> anyhow::Result<(IoCursor, Option<PendingBatchData>)> {
        let mut storage = self.pool.connection_tagged("state_keeper").await?;
        let cursor = IoCursor::new(&mut storage).await?;

        L2BlockSealProcess::clear_pending_l2_block(&mut storage, cursor.next_l2_block - 1).await?;

        let pending_l2_block_header = self
            .l1_batch_params_provider
            .load_first_l2_block_in_batch(&mut storage, cursor.l1_batch)
            .await
            .with_context(|| {
                format!(
                    "failed loading first L2 block for L1 batch #{}",
                    cursor.l1_batch
                )
            })?;
        let Some(pending_l2_block_header) = pending_l2_block_header else {
            return Ok((cursor, None));
        };

        let (system_env, l1_batch_env) = self
            .l1_batch_params_provider
            .load_l1_batch_params(
                &mut storage,
                &pending_l2_block_header,
                self.validation_computational_gas_limit,
                self.chain_id,
            )
            .await
            .with_context(|| format!("failed loading params for L1 batch #{}", cursor.l1_batch))?;
        let pending_batch_data = load_pending_batch(&mut storage, system_env, l1_batch_env)
            .await
            .with_context(|| {
                format!(
                    "failed loading data for re-execution for pending L1 batch #{}",
                    cursor.l1_batch
                )
            })?;

        let PendingBatchData {
            l1_batch_env,
            system_env,
            pending_l2_blocks,
        } = pending_batch_data;
        // Initialize the filter for the transactions that come after the pending batch.
        // We use values from the pending block to match the filter with one used before the restart.
        let (base_fee, gas_per_pubdata) =
            derive_base_fee_and_gas_per_pubdata(l1_batch_env.fee_input, system_env.version.into());
        self.filter = L2TxFilter {
            fee_input: l1_batch_env.fee_input,
            fee_per_gas: base_fee,
            gas_per_pubdata: gas_per_pubdata as u32,
        };

        Ok((
            cursor,
            Some(PendingBatchData {
                l1_batch_env,
                system_env,
                pending_l2_blocks,
            }),
        ))
    }

    async fn wait_for_new_batch_params(
        &mut self,
        cursor: &IoCursor,
        max_wait: Duration,
    ) -> anyhow::Result<Option<L1BatchParams>> {
        let deadline = Instant::now() + max_wait;

        // Block until at least one transaction in the mempool can match the filter (or timeout happens).
        // This is needed to ensure that block timestamp is not too old.
        for _ in 0..poll_iters(self.delay_interval, max_wait) {
            // We cannot create two L1 batches or L2 blocks with the same timestamp (forbidden by the bootloader).
            // Hence, we wait until the current timestamp is larger than the timestamp of the previous L2 block.
            // We can use `timeout_at` since `sleep_past` is cancel-safe; it only uses `sleep()` async calls.
            let timestamp = tokio::time::timeout_at(
                deadline.into(),
                sleep_past(cursor.prev_l2_block_timestamp, cursor.next_l2_block),
            );
            let Some(timestamp) = timestamp.await.ok() else {
                return Ok(None);
            };

            tracing::trace!(
                "Fee input for L1 batch #{} is {:#?}",
                cursor.l1_batch,
                self.filter.fee_input
            );
            let mut storage = self.pool.connection_tagged("state_keeper").await?;
            let protocol_version = storage
                .protocol_versions_dal()
                .protocol_version_id_by_timestamp(timestamp)
                .await
                .context("Failed loading protocol version")?;
            drop(storage);

            // We create a new filter each time, since parameters may change and a previously
            // ignored transaction in the mempool may be scheduled for the execution.
            self.filter = l2_tx_filter(
                self.batch_fee_input_provider.as_ref(),
                protocol_version.into(),
            )
            .await
            .context("failed creating L2 transaction filter")?;

            if !self.mempool.has_next(&self.filter) {
                tokio::time::sleep(self.delay_interval).await;
                continue;
            }

            return Ok(Some(L1BatchParams {
                protocol_version,
                validation_computational_gas_limit: self.validation_computational_gas_limit,
                operator_address: self.fee_account,
                fee_input: self.filter.fee_input,
                first_l2_block: L2BlockParams {
                    timestamp,
                    // This value is effectively ignored by the protocol.
                    virtual_blocks: 1,
                },
            }));
        }
        Ok(None)
    }

    async fn wait_for_new_l2_block_params(
        &mut self,
        cursor: &IoCursor,
        max_wait: Duration,
    ) -> anyhow::Result<Option<L2BlockParams>> {
        // We must provide different timestamps for each L2 block.
        // If L2 block sealing interval is greater than 1 second then `sleep_past` won't actually sleep.
        let timeout_result = tokio::time::timeout(
            max_wait,
            sleep_past(cursor.prev_l2_block_timestamp, cursor.next_l2_block),
        )
        .await;
        let Ok(timestamp) = timeout_result else {
            return Ok(None);
        };

        Ok(Some(L2BlockParams {
            timestamp,
            // This value is effectively ignored by the protocol.
            virtual_blocks: 1,
        }))
    }

    async fn wait_for_next_tx(
        &mut self,
        max_wait: Duration,
    ) -> anyhow::Result<Option<Transaction>> {
        let started_at = Instant::now();
        while started_at.elapsed() <= max_wait {
            let get_latency = KEEPER_METRICS.get_tx_from_mempool.start();
            let maybe_tx = self.mempool.next_transaction(&self.filter);
            get_latency.observe();

            if let Some(tx) = maybe_tx {
                // Reject transactions with too big gas limit. They are also rejected on the API level, but
                // we need to secure ourselves in case some tx will somehow get into mempool.
                if tx.gas_limit() > self.max_allowed_tx_gas_limit {
                    tracing::warn!(
                        "Found tx with too big gas limit in state keeper, hash: {:?}, gas_limit: {}",
                        tx.hash(),
                        tx.gas_limit()
                    );
                    self.reject(&tx, &Halt::TooBigGasLimit.to_string()).await?;
                    continue;
                }
                return Ok(Some(tx));
            } else {
                tokio::time::sleep(self.delay_interval).await;
                continue;
            }
        }
        Ok(None)
    }

    async fn rollback(&mut self, tx: Transaction) -> anyhow::Result<()> {
        // Reset nonces in the mempool.
        self.mempool.rollback(&tx);
        // Insert the transaction back.
        self.mempool.insert(vec![tx], HashMap::new());
        Ok(())
    }

    async fn reject(&mut self, rejected: &Transaction, error: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            !rejected.is_l1(),
            "L1 transactions should not be rejected: {error}"
        );

        // Reset the nonces in the mempool, but don't insert the transaction back.
        self.mempool.rollback(rejected);

        // Mark tx as rejected in the storage.
        let mut storage = self.pool.connection_tagged("state_keeper").await?;
        KEEPER_METRICS.rejected_transactions.inc();
        tracing::warn!(
            "Transaction {} is rejected with error: {error}",
            rejected.hash()
        );
        storage
            .transactions_dal()
            .mark_tx_as_rejected(rejected.hash(), &format!("rejected: {error}"))
            .await?;
        Ok(())
    }

    async fn load_base_system_contracts(
        &self,
        protocol_version: ProtocolVersionId,
        _cursor: &IoCursor,
    ) -> anyhow::Result<BaseSystemContracts> {
        self.pool
            .connection_tagged("state_keeper")
            .await?
            .protocol_versions_dal()
            .load_base_system_contracts_by_version_id(protocol_version as u16)
            .await
            .context("failed loading base system contracts")?
            .with_context(|| {
                format!(
                    "no base system contracts persisted for protocol version {protocol_version:?}"
                )
            })
    }

    async fn load_batch_version_id(
        &self,
        number: L1BatchNumber,
    ) -> anyhow::Result<ProtocolVersionId> {
        let mut storage = self.pool.connection_tagged("state_keeper").await?;
        self.l1_batch_params_provider
            .load_l1_batch_protocol_version(&mut storage, number)
            .await
            .with_context(|| format!("failed loading protocol version for L1 batch #{number}"))?
            .with_context(|| format!("L1 batch #{number} misses protocol version"))
    }

    async fn load_upgrade_tx(
        &self,
        version_id: ProtocolVersionId,
    ) -> anyhow::Result<Option<ProtocolUpgradeTx>> {
        let mut storage = self.pool.connection_tagged("state_keeper").await?;
        storage
            .protocol_versions_dal()
            .get_protocol_upgrade_tx(version_id)
            .await
            .map_err(Into::into)
    }

    async fn load_batch_state_hash(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<H256> {
        tracing::trace!("Getting L1 batch hash for L1 batch #{l1_batch_number}");
        let wait_latency = KEEPER_METRICS.wait_for_prev_hash_time.start();

        let mut storage = self.pool.connection_tagged("state_keeper").await?;
        let (batch_state_hash, _) = self
            .l1_batch_params_provider
            .wait_for_l1_batch_params(&mut storage, l1_batch_number)
            .await
            .with_context(|| format!("error waiting for params for L1 batch #{l1_batch_number}"))?;

        wait_latency.observe();
        tracing::trace!(
            "Got L1 batch state hash: {batch_state_hash:?} for L1 batch #{l1_batch_number}"
        );
        Ok(batch_state_hash)
    }
}

/// Sleeps until the current timestamp is larger than the provided `timestamp`.
///
/// Returns the current timestamp after the sleep. It is guaranteed to be larger than `timestamp`.
async fn sleep_past(timestamp: u64, l2_block: L2BlockNumber) -> u64 {
    let mut current_timestamp_millis = millis_since_epoch();
    let mut current_timestamp = (current_timestamp_millis / 1_000) as u64;
    match timestamp.cmp(&current_timestamp) {
        cmp::Ordering::Less => return current_timestamp,
        cmp::Ordering::Equal => {
            tracing::info!(
                "Current timestamp {} for L2 block #{l2_block} is equal to previous L2 block timestamp; waiting until \
                 timestamp increases",
                display_timestamp(current_timestamp)
            );
        }
        cmp::Ordering::Greater => {
            // This situation can be triggered if the system keeper is started on a pod with a different
            // system time, or if it is buggy. Thus, a one-time error could require no actions if L1 batches
            // are expected to be generated frequently.
            tracing::error!(
                "Previous L2 block timestamp {} is larger than the current timestamp {} for L2 block #{l2_block}",
                display_timestamp(timestamp),
                display_timestamp(current_timestamp)
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
    pub async fn new(
        mempool: MempoolGuard,
        batch_fee_input_provider: Arc<dyn BatchFeeModelInputProvider>,
        pool: ConnectionPool<Core>,
        config: &StateKeeperConfig,
        fee_account: Address,
        delay_interval: Duration,
        chain_id: L2ChainId,
    ) -> anyhow::Result<Self> {
        let mut storage = pool.connection_tagged("state_keeper").await?;
        let l1_batch_params_provider = L1BatchParamsProvider::new(&mut storage)
            .await
            .context("failed initializing L1 batch params provider")?;
        drop(storage);

        Ok(Self {
            mempool,
            pool,
            timeout_sealer: TimeoutSealer::new(config),
            l2_block_max_payload_size_sealer: L2BlockMaxPayloadSizeSealer::new(config),
            filter: L2TxFilter::default(),
            // ^ Will be initialized properly on the first newly opened batch
            l1_batch_params_provider,
            fee_account,
            validation_computational_gas_limit: config.validation_computational_gas_limit,
            max_allowed_tx_gas_limit: config.max_allowed_l2_tx_gas_limit.into(),
            delay_interval,
            batch_fee_input_provider,
            chain_id,
        })
    }
}

/// Getters required for testing the MempoolIO.
#[cfg(test)]
impl MempoolIO {
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
            timeout_at(deadline.into(), sleep_past(timestamp, L2BlockNumber(1)))
                .await
                .unwrap();
        }

        let current_timestamp = seconds_since_epoch();
        let deadline = Instant::now() + Duration::from_secs(2);
        let ts = timeout_at(
            deadline.into(),
            sleep_past(current_timestamp, L2BlockNumber(1)),
        )
        .await
        .unwrap();
        assert!(ts > current_timestamp);

        let future_timestamp = seconds_since_epoch() + 1;
        let deadline = Instant::now() + Duration::from_secs(3);
        let ts = timeout_at(
            deadline.into(),
            sleep_past(future_timestamp, L2BlockNumber(1)),
        )
        .await
        .unwrap();
        assert!(ts > future_timestamp);

        let future_timestamp = seconds_since_epoch() + 1;
        let deadline = Instant::now() + Duration::from_millis(100);
        // ^ This deadline is too small (we need at least 1_000ms)
        let result = timeout_at(
            deadline.into(),
            sleep_past(future_timestamp, L2BlockNumber(1)),
        )
        .await;
        assert!(result.is_err());
    }
}
