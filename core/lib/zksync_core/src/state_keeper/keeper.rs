use anyhow::Context as _;
use tokio::sync::watch;

use std::convert::Infallible;
use std::time::{Duration, Instant};

use multivm::interface::{Halt, L1BatchEnv, SystemEnv};
use zksync_types::{
    block::MiniblockExecutionData, l2::TransactionType, protocol_version::ProtocolUpgradeTx,
    storage_writes_deduplicator::StorageWritesDeduplicator, Transaction,
};

use super::{
    batch_executor::{BatchExecutorHandle, L1BatchExecutorBuilder, TxExecutionResult},
    extractors,
    io::{MiniblockParams, PendingBatchData, StateKeeperIO},
    metrics::{AGGREGATION_METRICS, KEEPER_METRICS, L1_BATCH_METRICS},
    seal_criteria::{ConditionalSealer, SealData, SealResolution},
    types::ExecutionMetricsForCriteria,
    updates::UpdatesManager,
};
use crate::gas_tracker::gas_count_from_writes;

/// Amount of time to block on waiting for some resource. The exact value is not really important,
/// we only need it to not block on waiting indefinitely and be able to process cancellation requests.
pub(super) const POLL_WAIT_DURATION: Duration = Duration::from_secs(1);

/// Structure used to indicate that task cancellation was requested.
#[derive(thiserror::Error, Debug)]
enum Error {
    #[error("canceled")]
    Canceled,
    #[error(transparent)]
    Fatal(#[from] anyhow::Error),
}

impl Error {
    fn context(self, msg: &'static str) -> Self {
        match self {
            Self::Canceled => Self::Canceled,
            Self::Fatal(err) => Self::Fatal(err.context(msg)),
        }
    }
}

/// State keeper represents a logic layer of batch/miniblock processing flow.
/// It's responsible for taking all the data from the `StateKeeperIO`, feeding it into `BatchExecutor` objects
/// and calling `SealManager` to decide whether miniblock or batch should be sealed.
///
/// State keeper maintains the batch execution state in the `UpdatesManager` until batch is sealed and these changes
/// are persisted by the `StateKeeperIO` implementation.
///
/// You can think of it as a state machine that runs over a sequence of incoming transactions, turning them into
/// a sequence of executed miniblocks and batches.
#[derive(Debug)]
pub struct ZkSyncStateKeeper {
    stop_receiver: watch::Receiver<bool>,
    io: Box<dyn StateKeeperIO>,
    batch_executor_base: Box<dyn L1BatchExecutorBuilder>,
    sealer: Option<ConditionalSealer>,
}

impl ZkSyncStateKeeper {
    pub fn new(
        stop_receiver: watch::Receiver<bool>,
        io: Box<dyn StateKeeperIO>,
        batch_executor_base: Box<dyn L1BatchExecutorBuilder>,
        sealer: ConditionalSealer,
    ) -> Self {
        Self {
            stop_receiver,
            io,
            batch_executor_base,
            sealer: Some(sealer),
        }
    }

    pub fn without_sealer(
        stop_receiver: watch::Receiver<bool>,
        io: Box<dyn StateKeeperIO>,
        batch_executor_base: Box<dyn L1BatchExecutorBuilder>,
    ) -> Self {
        Self {
            stop_receiver,
            io,
            batch_executor_base,
            sealer: None,
        }
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        match self.run_inner().await {
            Ok(_) => unreachable!(),
            Err(Error::Fatal(err)) => Err(err).context("state_keeper failed"),
            Err(Error::Canceled) => {
                tracing::info!("Stop signal received, state keeper is shutting down");
                Ok(())
            }
        }
    }

    /// Fallible version of `run` routine that allows to easily exit upon cancellation.
    async fn run_inner(&mut self) -> Result<Infallible, Error> {
        tracing::info!(
            "Starting state keeper. Next l1 batch to seal: {}, Next miniblock to seal: {}",
            self.io.current_l1_batch_number(),
            self.io.current_miniblock_number()
        );

        // Re-execute pending batch if it exists. Otherwise, initialize a new batch.
        let PendingBatchData {
            mut l1_batch_env,
            mut system_env,
            pending_miniblocks,
        } = match self.io.load_pending_batch().await {
            Some(params) => {
                tracing::info!(
                    "There exists a pending batch consisting of {} miniblocks, the first one is {}",
                    params.pending_miniblocks.len(),
                    params
                        .pending_miniblocks
                        .first()
                        .map(|miniblock| miniblock.number)
                        .context("Empty pending block represented as Some")?,
                );
                params
            }
            None => {
                tracing::info!("There is no open pending batch, starting a new empty batch");
                let (system_env, l1_batch_env) = self
                    .wait_for_new_batch_params()
                    .await
                    .map_err(|e| e.context("wait_for_new_batch_params()"))?;
                PendingBatchData {
                    l1_batch_env,
                    pending_miniblocks: Vec::new(),
                    system_env,
                }
            }
        };

        let protocol_version = system_env.version;
        let mut updates_manager = UpdatesManager::new(
            l1_batch_env.clone(),
            system_env.base_system_smart_contracts.hashes(),
            protocol_version,
        );

        let previous_batch_protocol_version =
            self.io.load_previous_batch_version_id().await.unwrap();
        let version_changed = protocol_version != previous_batch_protocol_version;

        let mut protocol_upgrade_tx = if pending_miniblocks.is_empty() && version_changed {
            self.io.load_upgrade_tx(protocol_version).await
        } else if !pending_miniblocks.is_empty() && version_changed {
            // Sanity check: if `txs_to_reexecute` is not empty and upgrade tx is present for this block
            // then it must be the first one in `txs_to_reexecute`.
            if self.io.load_upgrade_tx(protocol_version).await.is_some() {
                let first_tx_to_reexecute = &pending_miniblocks[0].txs[0];
                assert_eq!(
                    first_tx_to_reexecute.tx_format(),
                    TransactionType::ProtocolUpgradeTransaction
                )
            }

            None
        } else {
            None
        };

        let mut batch_executor = self
            .batch_executor_base
            .init_batch(l1_batch_env.clone(), system_env.clone())
            .await;

        self.restore_state(&batch_executor, &mut updates_manager, pending_miniblocks)
            .await?;

        let mut l1_batch_seal_delta: Option<Instant> = None;
        while !self.is_canceled() {
            // This function will run until the batch can be sealed.
            self.process_l1_batch(&batch_executor, &mut updates_manager, protocol_upgrade_tx)
                .await?;

            // Finish current batch.
            if !updates_manager.miniblock.executed_transactions.is_empty() {
                self.io.seal_miniblock(&updates_manager).await;
                // We've sealed the miniblock that we had, but we still need to setup the timestamp
                // for the fictive miniblock.
                let new_miniblock_params = self
                    .wait_for_new_miniblock_params(updates_manager.miniblock.timestamp)
                    .await?;
                Self::start_next_miniblock(
                    new_miniblock_params,
                    &mut updates_manager,
                    &batch_executor,
                )
                .await;
            }
            let (finished_batch, witness_block_state) = batch_executor.finish_batch().await;
            let sealed_batch_protocol_version = updates_manager.protocol_version();
            self.io
                .seal_l1_batch(
                    witness_block_state,
                    updates_manager,
                    &l1_batch_env,
                    finished_batch,
                )
                .await
                .context("seal_l1_batch")?;
            if let Some(delta) = l1_batch_seal_delta {
                L1_BATCH_METRICS.seal_delta.observe(delta.elapsed());
            }
            l1_batch_seal_delta = Some(Instant::now());

            // Start the new batch.
            (system_env, l1_batch_env) = self.wait_for_new_batch_params().await?;
            updates_manager = UpdatesManager::new(
                l1_batch_env.clone(),
                system_env.base_system_smart_contracts.hashes(),
                system_env.version,
            );
            batch_executor = self
                .batch_executor_base
                .init_batch(l1_batch_env.clone(), system_env.clone())
                .await;

            let version_changed = system_env.version != sealed_batch_protocol_version;

            protocol_upgrade_tx = if version_changed {
                self.io.load_upgrade_tx(system_env.version).await
            } else {
                None
            };
        }
        Err(Error::Canceled)
    }

    fn is_canceled(&self) -> bool {
        *self.stop_receiver.borrow()
    }

    async fn wait_for_new_batch_params(&mut self) -> Result<(SystemEnv, L1BatchEnv), Error> {
        while !self.is_canceled() {
            if let Some(params) = self.io.wait_for_new_batch_params(POLL_WAIT_DURATION).await {
                return Ok(params);
            }
        }
        Err(Error::Canceled)
    }

    async fn wait_for_new_miniblock_params(
        &mut self,
        prev_miniblock_timestamp: u64,
    ) -> Result<MiniblockParams, Error> {
        while !self.is_canceled() {
            if let Some(params) = self
                .io
                .wait_for_new_miniblock_params(POLL_WAIT_DURATION, prev_miniblock_timestamp)
                .await
            {
                return Ok(params);
            }
        }
        Err(Error::Canceled)
    }

    async fn start_next_miniblock(
        params: MiniblockParams,
        updates_manager: &mut UpdatesManager,
        batch_executor: &BatchExecutorHandle,
    ) {
        updates_manager.push_miniblock(params);
        batch_executor
            .start_next_miniblock(updates_manager.miniblock.get_miniblock_env())
            .await;
    }

    /// Applies the "pending state" on the `UpdatesManager`.
    /// Pending state means transactions that were executed before the server restart. Before we continue processing the
    /// batch, we need to restore the state. We must ensure that every transaction is executed successfully.
    ///
    /// Additionally, it initialized the next miniblock timestamp.
    async fn restore_state(
        &mut self,
        batch_executor: &BatchExecutorHandle,
        updates_manager: &mut UpdatesManager,
        miniblocks_to_reexecute: Vec<MiniblockExecutionData>,
    ) -> Result<(), Error> {
        if miniblocks_to_reexecute.is_empty() {
            return Ok(());
        }

        for (index, miniblock) in miniblocks_to_reexecute.into_iter().enumerate() {
            // Push any non-first miniblock to updates manager. The first one was pushed when `updates_manager` was initialized.
            if index > 0 {
                Self::start_next_miniblock(
                    MiniblockParams {
                        timestamp: miniblock.timestamp,
                        virtual_blocks: miniblock.virtual_blocks,
                    },
                    updates_manager,
                    batch_executor,
                )
                .await;
            }

            let miniblock_number = miniblock.number;
            tracing::info!(
                "Starting to reexecute transactions from sealed miniblock {}",
                miniblock_number
            );
            for tx in miniblock.txs {
                let result = batch_executor.execute_tx(tx.clone()).await;

                let TxExecutionResult::Success {
                    tx_result,
                    tx_metrics,
                    compressed_bytecodes,
                    call_tracer_result,
                    ..
                } = result
                else {
                    return Err(anyhow::anyhow!(
                        "Re-executing stored tx failed. Tx: {tx:?}. Err: {:?}",
                        result.err()
                    )
                    .into());
                };

                let ExecutionMetricsForCriteria {
                    l1_gas: tx_l1_gas_this_tx,
                    execution_metrics: tx_execution_metrics,
                } = tx_metrics;

                let tx_hash = tx.hash();
                let is_l1 = tx.is_l1();
                let exec_result_status = tx_result.result.clone();
                let initiator_account = tx.initiator_account();

                updates_manager.extend_from_executed_transaction(
                    tx,
                    *tx_result,
                    compressed_bytecodes,
                    tx_l1_gas_this_tx,
                    tx_execution_metrics,
                    call_tracer_result,
                );

                tracing::debug!(
                    "Finished re-executing tx {tx_hash} by {initiator_account} (is_l1: {is_l1}, \
                     #{idx_in_l1_batch} in L1 batch {l1_batch_number}, #{idx_in_miniblock} in miniblock {miniblock_number}); \
                     status: {exec_result_status:?}. L1 gas spent: {tx_l1_gas_this_tx:?}, total in L1 batch: {pending_l1_gas:?}, \
                     tx execution metrics: {tx_execution_metrics:?}, block execution metrics: {block_execution_metrics:?}",
                    idx_in_l1_batch = updates_manager.pending_executed_transactions_len(),
                    l1_batch_number = self.io.current_l1_batch_number().0,
                    idx_in_miniblock = updates_manager.miniblock.executed_transactions.len(),
                    pending_l1_gas = updates_manager.pending_l1_gas_count(),
                    block_execution_metrics = updates_manager.pending_execution_metrics()
                );
            }
        }

        // We've processed all the miniblocks, and right now we're initializing the next *actual* miniblock.
        let new_miniblock_params = self
            .wait_for_new_miniblock_params(updates_manager.miniblock.timestamp)
            .await
            .map_err(|e| e.context("wait_for_new_miniblock_params"))?;
        Self::start_next_miniblock(new_miniblock_params, updates_manager, batch_executor).await;

        Ok(())
    }

    async fn process_l1_batch(
        &mut self,
        batch_executor: &BatchExecutorHandle,
        updates_manager: &mut UpdatesManager,
        protocol_upgrade_tx: Option<ProtocolUpgradeTx>,
    ) -> Result<(), Error> {
        if let Some(protocol_upgrade_tx) = protocol_upgrade_tx {
            self.process_upgrade_tx(batch_executor, updates_manager, protocol_upgrade_tx)
                .await;
        }

        while !self.is_canceled() {
            if self
                .io
                .should_seal_l1_batch_unconditionally(updates_manager)
            {
                tracing::debug!(
                    "L1 batch #{} should be sealed unconditionally as per sealing rules",
                    self.io.current_l1_batch_number()
                );
                return Ok(());
            }

            if self.io.should_seal_miniblock(updates_manager) {
                tracing::debug!(
                    "Miniblock #{} (L1 batch #{}) should be sealed as per sealing rules",
                    self.io.current_miniblock_number(),
                    self.io.current_l1_batch_number()
                );
                self.io.seal_miniblock(updates_manager).await;

                let new_miniblock_params = self
                    .wait_for_new_miniblock_params(updates_manager.miniblock.timestamp)
                    .await
                    .map_err(|e| e.context("wait_for_new_miniblock_params"))?;
                tracing::debug!(
                    "Initialized new miniblock #{} (L1 batch #{}) with timestamp {}",
                    self.io.current_miniblock_number(),
                    self.io.current_l1_batch_number(),
                    extractors::display_timestamp(new_miniblock_params.timestamp)
                );
                Self::start_next_miniblock(new_miniblock_params, updates_manager, batch_executor)
                    .await;
            }

            let waiting_latency = KEEPER_METRICS.waiting_for_tx.start();
            let Some(tx) = self.io.wait_for_next_tx(POLL_WAIT_DURATION).await else {
                waiting_latency.observe();
                tracing::trace!("No new transactions. Waiting!");
                continue;
            };
            waiting_latency.observe();

            let tx_hash = tx.hash();
            let (seal_resolution, exec_result) = self
                .process_one_tx(batch_executor, updates_manager, tx.clone())
                .await;

            match &seal_resolution {
                SealResolution::NoSeal | SealResolution::IncludeAndSeal => {
                    let TxExecutionResult::Success {
                        tx_result,
                        tx_metrics,
                        call_tracer_result,
                        compressed_bytecodes,
                        ..
                    } = exec_result
                    else {
                        unreachable!(
                            "Tx inclusion seal resolution must be a result of a successful tx execution",
                        );
                    };
                    let ExecutionMetricsForCriteria {
                        l1_gas: tx_l1_gas_this_tx,
                        execution_metrics: tx_execution_metrics,
                    } = tx_metrics;
                    updates_manager.extend_from_executed_transaction(
                        tx,
                        *tx_result,
                        compressed_bytecodes,
                        tx_l1_gas_this_tx,
                        tx_execution_metrics,
                        call_tracer_result,
                    );
                }
                SealResolution::ExcludeAndSeal => {
                    batch_executor.rollback_last_tx().await;
                    self.io.rollback(tx).await;
                }
                SealResolution::Unexecutable(reason) => {
                    batch_executor.rollback_last_tx().await;
                    self.io.reject(&tx, reason).await;
                }
            };

            if seal_resolution.should_seal() {
                tracing::debug!(
                    "L1 batch #{} should be sealed with resolution {seal_resolution:?} after executing \
                     transaction {tx_hash}",
                    self.io.current_l1_batch_number()
                );
                return Ok(());
            }
        }
        Err(Error::Canceled)
    }

    async fn process_upgrade_tx(
        &mut self,
        batch_executor: &BatchExecutorHandle,
        updates_manager: &mut UpdatesManager,
        protocol_upgrade_tx: ProtocolUpgradeTx,
    ) {
        // Sanity check: protocol upgrade tx must be the first one in the batch.
        assert_eq!(updates_manager.pending_executed_transactions_len(), 0);

        let tx: Transaction = protocol_upgrade_tx.into();
        let (seal_resolution, exec_result) = self
            .process_one_tx(batch_executor, updates_manager, tx.clone())
            .await;

        match &seal_resolution {
            SealResolution::NoSeal | SealResolution::IncludeAndSeal => {
                let TxExecutionResult::Success {
                    tx_result,
                    tx_metrics,
                    compressed_bytecodes,
                    ..
                } = exec_result
                else {
                    panic!(
                        "Tx inclusion seal resolution must be a result of a successful tx execution",
                    );
                };

                // Despite success of upgrade transaction is not enforced by protocol,
                // we panic here because failed upgrade tx is not intended in any case.
                if tx_result.result.is_failed() {
                    panic!("Failed upgrade tx {:?}", tx.hash());
                }

                let ExecutionMetricsForCriteria {
                    l1_gas: tx_l1_gas_this_tx,
                    execution_metrics: tx_execution_metrics,
                    ..
                } = tx_metrics;
                updates_manager.extend_from_executed_transaction(
                    tx,
                    *tx_result,
                    compressed_bytecodes,
                    tx_l1_gas_this_tx,
                    tx_execution_metrics,
                    vec![],
                );
            }
            SealResolution::ExcludeAndSeal => {
                unreachable!("First tx in batch cannot result into `ExcludeAndSeal`");
            }
            SealResolution::Unexecutable(reason) => {
                panic!(
                    "Upgrade transaction {:?} is unexecutable: {}",
                    tx.hash(),
                    reason
                );
            }
        };
    }

    /// Executes one transaction in the batch executor, and then decides whether the batch should be sealed.
    /// Batch may be sealed because of one of the following reasons:
    /// 1. The VM entered an incorrect state (e.g. out of gas). In that case, we must revert the transaction and seal
    /// the block.
    /// 2. Seal manager decided that batch is ready to be sealed.
    /// Note: this method doesn't mutate `updates_manager` in the end. However, reference should be mutable
    /// because we use `apply_and_rollback` method of `updates_manager.storage_writes_deduplicator`.
    async fn process_one_tx(
        &mut self,
        batch_executor: &BatchExecutorHandle,
        updates_manager: &mut UpdatesManager,
        tx: Transaction,
    ) -> (SealResolution, TxExecutionResult) {
        let exec_result = batch_executor.execute_tx(tx.clone()).await;
        let resolution = match &exec_result {
            TxExecutionResult::BootloaderOutOfGasForTx => {
                AGGREGATION_METRICS
                    .inc("bootloader_tx_out_of_gas", &SealResolution::ExcludeAndSeal);
                SealResolution::ExcludeAndSeal
            }
            TxExecutionResult::BootloaderOutOfGasForBlockTip => {
                AGGREGATION_METRICS.inc(
                    "bootloader_block_tip_failed",
                    &SealResolution::ExcludeAndSeal,
                );
                SealResolution::ExcludeAndSeal
            }
            TxExecutionResult::RejectedByVm { reason } => match reason {
                Halt::NotEnoughGasProvided => {
                    AGGREGATION_METRICS.inc(
                        "not_enough_gas_provided_to_start_tx",
                        &SealResolution::ExcludeAndSeal,
                    );
                    SealResolution::ExcludeAndSeal
                }
                _ => SealResolution::Unexecutable(reason.to_string()),
            },
            TxExecutionResult::Success {
                tx_result,
                tx_metrics,
                bootloader_dry_run_metrics,
                bootloader_dry_run_result,
                ..
            } => {
                let tx_execution_status = &tx_result.result;
                let ExecutionMetricsForCriteria {
                    l1_gas: tx_l1_gas_this_tx,
                    execution_metrics: tx_execution_metrics,
                } = *tx_metrics;

                tracing::trace!(
                    "finished tx {:?} by {:?} (is_l1: {}) (#{} in l1 batch {}) (#{} in miniblock {}) \
                    status: {:?}. L1 gas spent: {:?}, total in l1 batch: {:?}, \
                    tx execution metrics: {:?}, block execution metrics: {:?}",
                    tx.hash(),
                    tx.initiator_account(),
                    tx.is_l1(),
                    updates_manager.pending_executed_transactions_len() + 1,
                    self.io.current_l1_batch_number().0,
                    updates_manager.miniblock.executed_transactions.len() + 1,
                    self.io.current_miniblock_number().0,
                    tx_execution_status,
                    tx_l1_gas_this_tx,
                    updates_manager.pending_l1_gas_count() + tx_l1_gas_this_tx,
                    &tx_execution_metrics,
                    updates_manager.pending_execution_metrics() + tx_execution_metrics,
                );

                let ExecutionMetricsForCriteria {
                    l1_gas: finish_block_l1_gas,
                    execution_metrics: finish_block_execution_metrics,
                } = *bootloader_dry_run_metrics;

                let encoding_len = tx.encoding_len();

                let logs_to_apply_iter = tx_result
                    .logs
                    .storage_logs
                    .iter()
                    .chain(&bootloader_dry_run_result.logs.storage_logs);
                let block_writes_metrics = updates_manager
                    .storage_writes_deduplicator
                    .apply_and_rollback(logs_to_apply_iter.clone());
                let block_writes_l1_gas = gas_count_from_writes(
                    &block_writes_metrics,
                    updates_manager.protocol_version(),
                );

                let tx_writes_metrics =
                    StorageWritesDeduplicator::apply_on_empty_state(logs_to_apply_iter);
                let tx_writes_l1_gas =
                    gas_count_from_writes(&tx_writes_metrics, updates_manager.protocol_version());
                let tx_gas_excluding_writes = tx_l1_gas_this_tx + finish_block_l1_gas;

                let tx_data = SealData {
                    execution_metrics: tx_execution_metrics + finish_block_execution_metrics,
                    gas_count: tx_gas_excluding_writes + tx_writes_l1_gas,
                    cumulative_size: encoding_len,
                    writes_metrics: tx_writes_metrics,
                };
                let block_data = SealData {
                    execution_metrics: tx_data.execution_metrics
                        + updates_manager.pending_execution_metrics(),
                    gas_count: tx_gas_excluding_writes
                        + block_writes_l1_gas
                        + updates_manager.pending_l1_gas_count(),
                    cumulative_size: tx_data.cumulative_size
                        + updates_manager.pending_txs_encoding_size(),
                    writes_metrics: block_writes_metrics,
                };

                if let Some(sealer) = &self.sealer {
                    sealer.should_seal_l1_batch(
                        self.io.current_l1_batch_number().0,
                        updates_manager.batch_timestamp() as u128 * 1_000,
                        updates_manager.pending_executed_transactions_len() + 1,
                        &block_data,
                        &tx_data,
                        updates_manager.protocol_version(),
                    )
                } else {
                    SealResolution::NoSeal
                }
            }
        };
        (resolution, exec_result)
    }
}
