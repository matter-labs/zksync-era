use std::{
    cmp,
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use async_trait::async_trait;
use zksync_config::configs::chain::StateKeeperConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_mempool::L2TxFilter;
use zksync_node_fee_model::BatchFeeModelInputProvider;
use zksync_state_keeper::{
    l2_tx_filter, metrics::KEEPER_METRICS, seal_criteria::UnexecutableReason, L2BlockParams,
    MempoolGuard,
};
use zksync_types::{
    block::UnsealedL1BatchHeader, utils::display_timestamp, Address, L2BlockNumber, L2ChainId,
    ProtocolVersionId, Transaction, U256,
};
use zksync_vm_interface::Halt;

use crate::{
    io::{
        seal_logic::l2_block_seal_subtasks::L2BlockSealProcess, BlockParams, IoCursor,
        StateKeeperIO,
    },
    millis_since_epoch,
    seal_criteria::{IoSealCriterion, TimeoutSealer},
    UpdatesManager,
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
    filter: L2TxFilter,
    fee_account: Address,
    validation_computational_gas_limit: u32,
    max_allowed_tx_gas_limit: U256,
    delay_interval: Duration,
    // Used to keep track of gas prices to set accepted price per pubdata byte in blocks.
    batch_fee_input_provider: Arc<dyn BatchFeeModelInputProvider>,
    chain_id: L2ChainId,
}

#[async_trait]
impl StateKeeperIO for MempoolIO {
    fn chain_id(&self) -> L2ChainId {
        self.chain_id
    }

    async fn initialize(&mut self) -> anyhow::Result<IoCursor> {
        let mut storage = self.pool.connection_tagged("state_keeper").await?;
        let cursor = IoCursor::new(&mut storage).await?;

        L2BlockSealProcess::clear_pending_l2_block(&mut storage, cursor.next_l2_block - 1).await?;

        Ok(cursor)
    }

    async fn wait_for_new_l2_block_params(
        &mut self,
        cursor: &IoCursor,
        max_wait: Duration,
    ) -> anyhow::Result<(Option<BlockParams>, Option<UnsealedL1BatchHeader>)> {
        // Check if there is an existing unsealed batch
        if let Some(unsealed_storage_batch) = self
            .pool
            .connection_tagged("state_keeper")
            .await?
            .blocks_dal()
            .get_unsealed_l1_batch_by_number(cursor.l1_batch)
            .await?
        {
            let protocol_version = unsealed_storage_batch
                .protocol_version
                .context("unsealed batch is missing protocol version")?;

            // TODO: zk os fee model
            let base_fee = unsealed_storage_batch.fee_input.fair_l2_gas_price();

            return Ok((
                Some(BlockParams {
                    // TODO: should timestamp_ms be persisted or is it ok to use `t * 1000`?
                    timestamp_ms: u128::from(unsealed_storage_batch.timestamp) * 1000,
                    fee_input: unsealed_storage_batch.fee_input,
                    protocol_version,
                    base_fee,
                }),
                None,
            ));
        }

        // Block until at least one transaction in the mempool can match the filter (or timeout happens).
        // This is needed to ensure that block timestamp is not too old.
        for _ in 0..poll_iters(self.delay_interval, max_wait) {
            let timestamp_ms = millis_since_epoch();

            // TODO: zk os protocol versions/upgrades
            let protocol_version = ProtocolVersionId::latest();

            tracing::trace!(
                "Fee input for L1 batch #{} is {:#?}",
                cursor.l1_batch,
                self.filter.fee_input
            );

            // We create a new filter each time, since parameters may change and a previously
            // ignored transaction in the mempool may be scheduled for the execution.
            self.filter = l2_tx_filter(
                self.batch_fee_input_provider.as_ref(),
                protocol_version.into(),
            )
            .await
            .context("failed creating L2 transaction filter")?;

            // We do not populate mempool with upgrade tx so it should be checked separately.
            if !self.mempool.has_next(&self.filter) {
                tokio::time::sleep(self.delay_interval).await;
                continue;
            }

            let header = UnsealedL1BatchHeader {
                number: cursor.l1_batch,
                timestamp: u64::try_from(timestamp_ms / 1000).unwrap(),
                protocol_version: Some(protocol_version),
                fee_address: self.fee_account,
                fee_input: self.filter.fee_input,
            };
            let block_params = BlockParams {
                timestamp_ms,
                fee_input: self.filter.fee_input,
                protocol_version: ProtocolVersionId::latest(),
                base_fee: self.filter.fee_per_gas,
            };

            return Ok((Some(block_params), Some(header)));
        }
        Ok((None, None))
    }

    async fn wait_for_next_tx(
        &mut self,
        max_wait: Duration,
        l2_block_timestamp: u64,
    ) -> anyhow::Result<Option<Transaction>> {
        let started_at = Instant::now();
        while started_at.elapsed() <= max_wait {
            let get_latency = KEEPER_METRICS.get_tx_from_mempool.start();
            let maybe_tx = self.mempool.next_transaction(&self.filter);
            get_latency.observe();

            if let Some((tx, constraint)) = maybe_tx {
                // Reject transactions with too big gas limit. They are also rejected on the API level, but
                // we need to secure ourselves in case some tx will somehow get into mempool.
                if tx.gas_limit() > self.max_allowed_tx_gas_limit {
                    tracing::warn!(
                        "Found tx with too big gas limit in state keeper, hash: {:?}, gas_limit: {}",
                        tx.hash(),
                        tx.gas_limit()
                    );
                    self.reject(&tx, UnexecutableReason::Halt(Halt::TooBigGasLimit))
                        .await?;
                    continue;
                }

                // Reject transactions that violate block.timestamp constraints. Such transactions should be
                // rejected at the API level, but we need to protect ourselves in case if a transaction
                // goes outside of the allowed range while being in the mempool
                let matches_range = constraint
                    .timestamp_asserter_range
                    .map_or(true, |x| x.contains(&l2_block_timestamp));

                if !matches_range {
                    self.reject(
                        &tx,
                        UnexecutableReason::Halt(Halt::FailedBlockTimestampAssertion),
                    )
                    .await?;
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
        let constraint = self.mempool.rollback(&tx);
        // Insert the transaction back.
        self.mempool.insert(vec![(tx, constraint)], HashMap::new());
        Ok(())
    }

    async fn reject(
        &mut self,
        rejected: &Transaction,
        reason: UnexecutableReason,
    ) -> anyhow::Result<()> {
        let hash = rejected.hash();
        anyhow::ensure!(
            !rejected.is_l1(),
            "L1 transactions should not be rejected: {hash}"
        );

        // Mark tx as rejected in the storage.
        let mut storage = self.pool.connection_tagged("state_keeper").await?;

        KEEPER_METRICS.inc_rejected_txs(reason.as_metric_label());

        tracing::warn!(
            "Transaction {} is rejected with error: {reason}",
            rejected.hash()
        );
        storage
            .transactions_dal()
            .mark_tx_as_rejected(rejected.hash(), &format!("rejected: {reason}"))
            .await?;

        // In-memory pool should be updated strictly after DB is, so the rejected tx cannot be re-inserted there.
        // Reset the nonces in the mempool, but don't insert the transaction back.
        self.mempool.rollback(rejected);

        Ok(())
    }
}

impl IoSealCriterion for MempoolIO {
    fn should_seal_block(&mut self, manager: &UpdatesManager) -> bool {
        if self.timeout_sealer.should_seal_block(manager) {
            return true;
        }

        false
    }
}

impl MempoolIO {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mempool: MempoolGuard,
        batch_fee_input_provider: Arc<dyn BatchFeeModelInputProvider>,
        pool: ConnectionPool<Core>,
        config: &StateKeeperConfig,
        fee_account: Address,
        delay_interval: Duration,
        chain_id: L2ChainId,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            mempool,
            pool,
            timeout_sealer: TimeoutSealer::new(&config),
            filter: L2TxFilter::default(),
            // ^ Will be initialized properly on the first newly opened batch
            fee_account,
            validation_computational_gas_limit: config.validation_computational_gas_limit,
            max_allowed_tx_gas_limit: config.max_allowed_l2_tx_gas_limit.into(),
            delay_interval,
            batch_fee_input_provider,
            chain_id,
        })
    }
}

/// Returns the amount of iterations `delay_interval` fits into `max_wait`, rounding up.
fn poll_iters(delay_interval: Duration, max_wait: Duration) -> usize {
    let max_wait_millis = max_wait.as_millis() as u64;
    let delay_interval_millis = delay_interval.as_millis() as u64;
    assert!(delay_interval_millis > 0, "delay interval must be positive");

    ((max_wait_millis + delay_interval_millis - 1) / delay_interval_millis).max(1) as usize
}
