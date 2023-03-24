use std::time::Duration;

use tokio::sync::watch::Receiver;

use vm::transaction_data::TransactionData;
use vm::TxRevertReason;
use zksync_types::{
    storage_writes_deduplicator::StorageWritesDeduplicator, MiniblockNumber, Transaction,
};
use zksync_utils::time::millis_since_epoch;

use crate::gas_tracker::gas_count_from_writes;
use crate::state_keeper::{
    batch_executor::{BatchExecutorHandle, L1BatchExecutorBuilder, TxExecutionResult},
    io::{L1BatchParams, PendingBatchData, StateKeeperIO},
    seal_criteria::{SealManager, SealResolution},
    types::ExecutionMetricsForCriteria,
    updates::UpdatesManager,
};

/// Amount of time to block on waiting for some resource. The exact value is not really important,
/// we only need it to not block on waiting indefinitely and be able to process cancellation requests.
pub(super) const POLL_WAIT_DURATION: Duration = Duration::from_secs(1);

/// Structure used to indicate that task cancellation was requested.
#[derive(Debug)]
struct Canceled;

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
    stop_receiver: Receiver<bool>,
    io: Box<dyn StateKeeperIO>,
    batch_executor_base: Box<dyn L1BatchExecutorBuilder>,
    sealer: SealManager,
}

impl ZkSyncStateKeeper {
    pub fn new(
        stop_receiver: Receiver<bool>,
        io: Box<dyn StateKeeperIO>,
        batch_executor_base: Box<dyn L1BatchExecutorBuilder>,
        sealer: SealManager,
    ) -> Self {
        ZkSyncStateKeeper {
            stop_receiver,
            io,
            batch_executor_base,
            sealer,
        }
    }

    pub fn run(mut self) {
        match self.run_inner() {
            Ok(()) => {
                // Normally, state keeper can only exit its routine if the task was cancelled.
                panic!("State keeper exited the main loop")
            }
            Err(Canceled) => {
                vlog::info!("Stop signal received, state keeper is shutting down");
            }
        }
    }

    /// Fallible version of `run` routine that allows to easily exit upon cancellation.
    fn run_inner(&mut self) -> Result<(), Canceled> {
        vlog::info!(
            "Starting state keeper. Next l1 batch to seal: {}, Next miniblock to seal: {}",
            self.io.current_l1_batch_number(),
            self.io.current_miniblock_number()
        );

        // Re-execute pending batch if it exists. Otherwise, initialize a new batch.
        let PendingBatchData {
            params,
            txs: txs_to_reexecute,
        } = match self.io.load_pending_batch() {
            Some(params) => {
                vlog::info!(
                    "There exists a pending batch consisting of {} miniblocks, the first one is {}",
                    params.txs.len(),
                    params
                        .txs
                        .first()
                        .map(|(number, _)| number)
                        .expect("Empty pending block represented as Some")
                );
                params
            }
            None => {
                vlog::info!("There is no open pending batch, starting a new empty batch");
                PendingBatchData {
                    params: self.wait_for_new_batch_params()?,
                    txs: Vec::new(),
                }
            }
        };

        let mut l1_batch_params = params;

        let mut updates_manager = UpdatesManager::new(
            &l1_batch_params.context_mode,
            l1_batch_params.base_system_contracts.hashes(),
        );

        let mut batch_executor = self.batch_executor_base.init_batch(l1_batch_params.clone());
        self.restore_state(&batch_executor, &mut updates_manager, txs_to_reexecute);

        loop {
            self.check_if_cancelled()?;

            // This function will run until the batch can be sealed.
            self.process_l1_batch(&batch_executor, &mut updates_manager)?;

            // Finish current batch.
            if !updates_manager.miniblock.executed_transactions.is_empty() {
                self.io.seal_miniblock(&updates_manager);
                // We've sealed the miniblock that we had, but we still need to setup the timestamp for the
                // fictive miniblock.
                let fictive_miniblock_timestamp = self.wait_for_new_miniblock_params()?;
                updates_manager.seal_miniblock(fictive_miniblock_timestamp);
            }
            let block_result = batch_executor.finish_batch();
            self.io.seal_l1_batch(
                block_result,
                updates_manager,
                l1_batch_params.context_mode.inner_block_context(),
            );

            // Start the new batch.
            l1_batch_params = self.wait_for_new_batch_params()?;
            updates_manager = UpdatesManager::new(
                &l1_batch_params.context_mode,
                l1_batch_params.base_system_contracts.hashes(),
            );
            batch_executor = self.batch_executor_base.init_batch(l1_batch_params.clone());
        }
    }

    fn check_if_cancelled(&self) -> Result<(), Canceled> {
        if *self.stop_receiver.borrow() {
            return Err(Canceled);
        }
        Ok(())
    }

    fn wait_for_new_batch_params(&mut self) -> Result<L1BatchParams, Canceled> {
        let params = loop {
            if let Some(params) = self.io.wait_for_new_batch_params(POLL_WAIT_DURATION) {
                break params;
            }
            self.check_if_cancelled()?;
        };
        Ok(params)
    }

    fn wait_for_new_miniblock_params(&mut self) -> Result<u64, Canceled> {
        let params = loop {
            if let Some(params) = self.io.wait_for_new_miniblock_params(POLL_WAIT_DURATION) {
                break params;
            }
            self.check_if_cancelled()?;
        };
        Ok(params)
    }

    /// Applies the "pending state" on the `UpdatesManager`.
    /// Pending state means transactions that were executed before the server restart. Before we continue processing the
    /// batch, we need to restore the state. We must ensure that every transaction is executed successfully.
    fn restore_state(
        &mut self,
        batch_executor: &BatchExecutorHandle,
        updates_manager: &mut UpdatesManager,
        txs_to_reexecute: Vec<(MiniblockNumber, Vec<Transaction>)>,
    ) {
        for (miniblock_number, txs) in txs_to_reexecute {
            vlog::info!(
                "Starting to reexecute transactions from sealed miniblock {}",
                miniblock_number
            );
            for tx in txs {
                let result = batch_executor.execute_tx(tx.clone());

                if !result.success() {
                    let err = result.err().unwrap();
                    panic!(
                        "Re-executing stored tx failed. Tx: {:?}. Err: {:?}",
                        tx, err
                    )
                };
                let tx_execution_result = result.tx_result.unwrap();
                let tx_execution_status = tx_execution_result.status;

                let ExecutionMetricsForCriteria {
                    l1_gas: tx_l1_gas_this_tx,
                    execution_metrics: tx_execution_metrics,
                } = result.tx_metrics.unwrap();

                updates_manager.extend_from_executed_transaction(
                    &tx,
                    tx_execution_result,
                    result.compressed_bytecodes,
                    tx_l1_gas_this_tx,
                    tx_execution_metrics,
                );
                vlog::debug!(
                    "finished reexecuting tx {} by {} (is_l1: {}) (#{} in l1 batch {}) \
                    (#{} in miniblock {}) status: {:?}. L1 gas spent: {:?}, total in l1 batch: {:?}, \
                    tx execution metrics: {:?}, block execution metrics: {:?}",
                    tx.hash(),
                    tx.initiator_account(),
                    tx.is_l1(),
                    updates_manager.pending_executed_transactions_len(),
                    self.io.current_l1_batch_number().0,
                    updates_manager.miniblock.executed_transactions.len(),
                    miniblock_number,
                    tx_execution_status,
                    tx_l1_gas_this_tx,
                    updates_manager.pending_l1_gas_count(),
                    &tx_execution_metrics,
                    updates_manager.pending_execution_metrics(),
                );
            }

            // For old miniblocks that we reexecute the correct timestamps are already persisted in the DB and won't be overwritten.
            // However, `seal_miniblock` method of `UpdatesManager` takes the only parameter `new_miniblock_timstamp`
            // that will be used as a timestamp for the next sealed miniblock.
            // So, we should care about passing the correct timestamp for miniblock that comes after the pending batch.
            updates_manager.seal_miniblock((millis_since_epoch() / 1000) as u64);
        }
    }

    fn process_l1_batch(
        &mut self,
        batch_executor: &BatchExecutorHandle,
        updates_manager: &mut UpdatesManager,
    ) -> Result<(), Canceled> {
        loop {
            self.check_if_cancelled()?;
            if self
                .sealer
                .should_seal_l1_batch_unconditionally(updates_manager)
            {
                return Ok(());
            }
            if self.sealer.should_seal_miniblock(updates_manager) {
                self.io.seal_miniblock(updates_manager);
                let new_timestamp = self.wait_for_new_miniblock_params()?;
                updates_manager.seal_miniblock(new_timestamp);
            }
            let Some(tx) = self.io.wait_for_next_tx(POLL_WAIT_DURATION) else {
                vlog::trace!("No new transactions. Waiting!");
                continue;
            };

            let (seal_resolution, exec_result) =
                self.process_one_tx(batch_executor, updates_manager, &tx);

            match &seal_resolution {
                SealResolution::NoSeal => {
                    let ExecutionMetricsForCriteria {
                        l1_gas: tx_l1_gas_this_tx,
                        execution_metrics: tx_execution_metrics,
                        ..
                    } = exec_result.tx_metrics.unwrap();
                    updates_manager.extend_from_executed_transaction(
                        &tx,
                        exec_result.tx_result.unwrap(),
                        exec_result.compressed_bytecodes,
                        tx_l1_gas_this_tx,
                        tx_execution_metrics,
                    );
                }
                SealResolution::IncludeAndSeal => {
                    let ExecutionMetricsForCriteria {
                        l1_gas: tx_l1_gas_this_tx,
                        execution_metrics: tx_execution_metrics,
                        ..
                    } = exec_result.tx_metrics.unwrap();
                    updates_manager.extend_from_executed_transaction(
                        &tx,
                        exec_result.tx_result.unwrap(),
                        exec_result.compressed_bytecodes,
                        tx_l1_gas_this_tx,
                        tx_execution_metrics,
                    );
                }
                SealResolution::ExcludeAndSeal => {
                    batch_executor.rollback_last_tx();
                    self.io.rollback(&tx);
                }
                SealResolution::Unexecutable(reason) => {
                    batch_executor.rollback_last_tx();
                    self.io.reject(&tx, reason);
                }
            };

            if seal_resolution.should_seal() {
                return Ok(());
            }
        }
    }

    /// Executes one transaction in the batch executor, and then decides whether the batch should be sealed.
    /// Batch may be sealed because of one of the following reasons:
    /// 1. The VM entered an incorrect state (e.g. out of gas). In that case, we must revert the transaction and seal
    /// the block.
    /// 2. Seal manager decided that batch is ready to be sealed.
    /// Note: this method doesn't mutate `updates_manager` in the end. However, reference should be mutable
    /// because we use `apply_and_rollback` method of `updates_manager.storage_writes_deduplicator`.
    fn process_one_tx(
        &mut self,
        batch_executor: &BatchExecutorHandle,
        updates_manager: &mut UpdatesManager,
        tx: &Transaction,
    ) -> (SealResolution, TxExecutionResult) {
        let exec_result = batch_executor.execute_tx(tx.clone());
        let TxExecutionResult {
            tx_result,
            bootloader_dry_run_result,
            tx_metrics,
            bootloader_dry_run_metrics,
            ..
        } = exec_result.clone();

        match tx_result {
            Err(TxRevertReason::BootloaderOutOfGas) => {
                metrics::increment_counter!(
                    "server.tx_aggregation.reason",
                    "criterion" => "bootloader_tx_out_of_gas",
                    "seal_resolution" => "exclude_and_seal",
                );
                (SealResolution::ExcludeAndSeal, exec_result)
            }
            Err(rejection) => (
                SealResolution::Unexecutable(rejection.to_string()),
                exec_result,
            ),
            Ok(tx_execution_result) => {
                let tx_execution_status = tx_execution_result.status;
                let ExecutionMetricsForCriteria {
                    l1_gas: tx_l1_gas_this_tx,
                    execution_metrics: tx_execution_metrics,
                } = tx_metrics.unwrap();

                vlog::debug!(
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

                let bootloader_dry_run_result =
                    if let Ok(bootloader_dry_run_result) = bootloader_dry_run_result.unwrap() {
                        bootloader_dry_run_result
                    } else {
                        // Exclude and seal.
                        metrics::increment_counter!(
                            "server.tx_aggregation.reason",
                            "criterion" => "bootloader_block_tip_failed",
                            "seal_resolution" => "exclude_and_seal",
                        );
                        return (SealResolution::ExcludeAndSeal, exec_result);
                    };

                let ExecutionMetricsForCriteria {
                    l1_gas: finish_block_l1_gas,
                    execution_metrics: finish_block_execution_metrics,
                    ..
                } = bootloader_dry_run_metrics.unwrap();

                let tx_data: TransactionData = tx.clone().into();
                let encoding_len = tx_data.into_tokens().len();

                let logs_to_apply_iter = tx_execution_result
                    .result
                    .logs
                    .storage_logs
                    .iter()
                    .chain(&bootloader_dry_run_result.logs.storage_logs);
                let block_writes_metrics = updates_manager
                    .storage_writes_deduplicator
                    .apply_and_rollback(logs_to_apply_iter.clone());
                let block_writes_l1_gas = gas_count_from_writes(&block_writes_metrics);

                let tx_writes_metrics =
                    StorageWritesDeduplicator::apply_on_empty_state(logs_to_apply_iter);
                let tx_writes_l1_gas = gas_count_from_writes(&tx_writes_metrics);

                let resolution = self.sealer.should_seal_l1_batch(
                    self.io.current_l1_batch_number().0,
                    updates_manager.batch_timestamp() as u128 * 1000,
                    updates_manager.pending_executed_transactions_len() + 1,
                    updates_manager.pending_execution_metrics()
                        + tx_execution_metrics
                        + finish_block_execution_metrics,
                    tx_execution_metrics + finish_block_execution_metrics,
                    updates_manager.pending_l1_gas_count()
                        + tx_l1_gas_this_tx
                        + finish_block_l1_gas
                        + block_writes_l1_gas,
                    tx_l1_gas_this_tx + finish_block_l1_gas + tx_writes_l1_gas,
                    updates_manager.pending_txs_encoding_size() + encoding_len,
                    encoding_len,
                    block_writes_metrics,
                    tx_writes_metrics,
                );

                (resolution, exec_result)
            }
        }
    }
}
