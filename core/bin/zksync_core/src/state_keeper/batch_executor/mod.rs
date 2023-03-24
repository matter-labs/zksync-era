use std::{sync::mpsc, thread, time::Instant};

use vm::{
    storage::Storage,
    vm::{VmPartialExecutionResult, VmTxExecutionResult},
    vm_with_bootloader::{
        init_vm, init_vm_with_gas_limit, push_transaction_to_bootloader_memory, BootloaderJobType,
        TxExecutionMode,
    },
    TxRevertReason, VmBlockResult, VmInstance,
};
use zksync_dal::ConnectionPool;
use zksync_state::{secondary_storage::SecondaryStateStorage, storage_view::StorageView};
use zksync_storage::{db::Database, RocksDB};
use zksync_types::{tx::ExecutionMetrics, Transaction, U256};
use zksync_utils::bytecode::{hash_bytecode, CompressedBytecodeInfo};

use crate::gas_tracker::{gas_count_from_metrics, gas_count_from_tx_and_metrics};
use crate::state_keeper::io::L1BatchParams;

use crate::state_keeper::types::ExecutionMetricsForCriteria;

#[cfg(test)]
mod tests;

/// Representation of a transaction executed in the virtual machine.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TxExecutionResult {
    /// `Ok(_)` represents a transaction that was executed (even if it reverted), while
    /// `Err(_)` represents a rejected transaction (one that can't be applied to the state).
    pub(super) tx_result: Result<VmTxExecutionResult, TxRevertReason>,
    /// Result of dry run executing the bootloader tip. Will be `None` if the transaction was rejected
    /// (`tx_result` field is `err).
    pub(super) bootloader_dry_run_result: Option<Result<VmPartialExecutionResult, TxRevertReason>>,
    /// Execution metrics for the transaction itself.
    /// Will be `None` if the transaction was rejected.
    pub(super) tx_metrics: Option<ExecutionMetricsForCriteria>,
    /// Execution metrics for the bootloader tip dry run.
    /// Will be `None` if either the transaction was rejected or if bootloader tip dry run failed.
    pub(super) bootloader_dry_run_metrics: Option<ExecutionMetricsForCriteria>,
    /// Bytecodes that are to be published as compressed with this transaction
    pub(super) compressed_bytecodes: Vec<CompressedBytecodeInfo>,
}

impl TxExecutionResult {
    pub(crate) fn new(
        tx_result: Result<(VmTxExecutionResult, Vec<CompressedBytecodeInfo>), TxRevertReason>,
    ) -> Self {
        let (tx_result, compressed_bytecodes) = match tx_result {
            Ok((result, compressed_bytecodes)) => (Ok(result), compressed_bytecodes),
            Err(err) => (Err(err), vec![]),
        };

        Self {
            tx_result,
            bootloader_dry_run_result: None,
            tx_metrics: None,
            bootloader_dry_run_metrics: None,
            compressed_bytecodes,
        }
    }

    pub(crate) fn add_tx_metrics(&mut self, tx_metrics: ExecutionMetricsForCriteria) {
        self.tx_metrics = Some(tx_metrics);
    }

    pub(crate) fn add_bootloader_result(
        &mut self,
        bootloader_dry_run_result: Result<VmPartialExecutionResult, TxRevertReason>,
    ) {
        self.bootloader_dry_run_result = Some(bootloader_dry_run_result);
    }

    pub(crate) fn add_bootloader_metrics(
        &mut self,
        bootloader_dry_run_metrics: ExecutionMetricsForCriteria,
    ) {
        self.bootloader_dry_run_metrics = Some(bootloader_dry_run_metrics);
    }

    /// Returns `true` if both transaction and bootloader tip dry run were successful.
    pub(super) fn success(&self) -> bool {
        self.tx_result.is_ok()
            && self
                .bootloader_dry_run_result
                .as_ref()
                .map(|res| res.is_ok())
                .unwrap_or(false)
    }

    /// Returns a revert reason if either transaction was rejected or bootloader dry run tip failed.
    pub(super) fn err(&self) -> Option<TxRevertReason> {
        self.tx_result
            .as_ref()
            .err()
            .or_else(|| {
                self.bootloader_dry_run_result
                    .as_ref()
                    .and_then(|res| res.as_ref().err())
            })
            .cloned()
    }
}

/// An abstraction that allows us to create different kinds of batch executors.
/// The only requirement is to return the `BatchExecutorHandle` object, which does its work
/// by communicating with the externally initialized thread.
pub trait L1BatchExecutorBuilder: 'static + std::fmt::Debug + Send {
    fn init_batch(&self, l1_batch_params: L1BatchParams) -> BatchExecutorHandle;
}

/// The default implementation of the `BatchExecutorBuilder`.
/// Creates a "real" batch executor which maintains the VM (as opposed to the test builder which doesn't use the VM).
#[derive(Debug, Clone)]
pub(crate) struct MainBatchExecutorBuilder {
    state_keeper_db_path: String,
    pool: ConnectionPool,
    reexecute_each_tx: bool,
    max_allowed_tx_gas_limit: U256,
    validation_computational_gas_limit: u32,
}

impl MainBatchExecutorBuilder {
    pub(crate) fn new(
        state_keeper_db_path: String,
        pool: ConnectionPool,
        reexecute_each_tx: bool,
        max_allowed_tx_gas_limit: U256,
        validation_computational_gas_limit: u32,
    ) -> Self {
        Self {
            state_keeper_db_path,
            pool,
            reexecute_each_tx,
            max_allowed_tx_gas_limit,
            validation_computational_gas_limit,
        }
    }
}

impl L1BatchExecutorBuilder for MainBatchExecutorBuilder {
    fn init_batch(&self, l1_batch_params: L1BatchParams) -> BatchExecutorHandle {
        let secondary_storage = self
            .pool
            .access_storage_blocking()
            .storage_load_dal()
            .load_secondary_storage(RocksDB::new(
                Database::StateKeeper,
                &self.state_keeper_db_path,
                true,
            ));
        vlog::info!(
            "Secondary storage for batch {} initialized, size is {}",
            l1_batch_params
                .context_mode
                .inner_block_context()
                .context
                .block_number,
            secondary_storage.get_estimated_map_size()
        );
        metrics::gauge!(
            "server.state_keeper.storage_map_size",
            secondary_storage.get_estimated_map_size() as f64,
        );
        BatchExecutorHandle::new(
            self.reexecute_each_tx,
            self.max_allowed_tx_gas_limit,
            self.validation_computational_gas_limit,
            secondary_storage,
            l1_batch_params,
            None,
        )
    }
}

/// A public interface for interaction with the `BatchExecutor`.
/// `BatchExecutorHandle` is stored in the state keeper and is used to invoke or rollback transactions, and also seal
/// the batches.
#[derive(Debug)]
pub struct BatchExecutorHandle {
    handle: thread::JoinHandle<()>,
    commands: mpsc::Sender<Command>,
}

impl BatchExecutorHandle {
    pub(super) fn new(
        reexecute_each_tx: bool,
        max_allowed_tx_gas_limit: U256,
        validation_computational_gas_limit: u32,
        secondary_storage: SecondaryStateStorage,
        l1_batch_params: L1BatchParams,
        vm_gas_limit: Option<u32>,
    ) -> Self {
        let (commands_sender, commands_receiver) = mpsc::channel();
        let executor = BatchExecutor {
            reexecute_each_tx,
            max_allowed_tx_gas_limit,
            validation_computational_gas_limit,
            commands: commands_receiver,
            vm_gas_limit,
        };

        let handle = thread::spawn(move || executor.run(secondary_storage, l1_batch_params));

        Self {
            handle,
            commands: commands_sender,
        }
    }

    /// Creates a batch executor handle from the provided sender and thread join handle.
    /// Can be used to inject an alternative batch executor implementation.
    pub(crate) fn from_raw(
        handle: thread::JoinHandle<()>,
        commands: mpsc::Sender<Command>,
    ) -> Self {
        Self { handle, commands }
    }

    pub(super) fn execute_tx(&self, tx: Transaction) -> TxExecutionResult {
        let (response_sender, response_receiver) = mpsc::sync_channel(0);
        self.commands
            .send(Command::ExecuteTx(tx, response_sender))
            .unwrap();

        let start = Instant::now();
        let res = response_receiver.recv().unwrap();
        metrics::histogram!("state_keeper.batch_executor.command_response_time", start.elapsed(), "command" => "execute_tx");
        res
    }

    pub(super) fn rollback_last_tx(&self) {
        // While we don't get anything from the channel, it's useful to have it as a confirmation that the operation
        // indeed has been processed.
        let (response_sender, response_receiver) = mpsc::sync_channel(0);
        self.commands
            .send(Command::RollbackLastTx(response_sender))
            .unwrap();
        let start = Instant::now();
        response_receiver.recv().unwrap();
        metrics::histogram!("state_keeper.batch_executor.command_response_time", start.elapsed(), "command" => "rollback_last_tx");
    }

    pub(super) fn finish_batch(self) -> VmBlockResult {
        let (response_sender, response_receiver) = mpsc::sync_channel(0);
        self.commands
            .send(Command::FinishBatch(response_sender))
            .unwrap();
        let start = Instant::now();
        let resp = response_receiver.recv().unwrap();
        self.handle.join().unwrap();
        metrics::histogram!("state_keeper.batch_executor.command_response_time", start.elapsed(), "command" => "finish_batch");
        resp
    }
}

/// Implementation of the "primary" (non-test) batch executor.
/// Upon launch, it initializes the VM object with provided block context and properties, and keeps applying
/// transactions until the batch is sealed.
///
/// One `BatchExecutor` can execute exactly one batch, so once the batch is sealed, a new `BatchExecutor` object must
/// be constructed.
#[derive(Debug)]
pub(super) struct BatchExecutor {
    reexecute_each_tx: bool,
    max_allowed_tx_gas_limit: U256,
    validation_computational_gas_limit: u32,
    commands: mpsc::Receiver<Command>,
    vm_gas_limit: Option<u32>,
}

#[allow(clippy::large_enum_variant)]
pub(crate) enum Command {
    ExecuteTx(Transaction, mpsc::SyncSender<TxExecutionResult>),
    RollbackLastTx(mpsc::SyncSender<()>),
    FinishBatch(mpsc::SyncSender<VmBlockResult>),
}

impl BatchExecutor {
    pub(super) fn run(
        self,
        secondary_storage: SecondaryStateStorage,
        l1_batch_params: L1BatchParams,
    ) {
        vlog::info!(
            "Starting executing batch #{}",
            l1_batch_params
                .context_mode
                .inner_block_context()
                .context
                .block_number
        );

        let mut storage_view = StorageView::new(&secondary_storage);
        let mut oracle_tools = vm::OracleTools::new(&mut storage_view as &mut dyn Storage);

        let mut vm = match self.vm_gas_limit {
            Some(vm_gas_limit) => init_vm_with_gas_limit(
                &mut oracle_tools,
                l1_batch_params.context_mode,
                &l1_batch_params.properties,
                TxExecutionMode::VerifyExecute,
                &l1_batch_params.base_system_contracts,
                vm_gas_limit,
            ),
            None => init_vm(
                &mut oracle_tools,
                l1_batch_params.context_mode,
                &l1_batch_params.properties,
                TxExecutionMode::VerifyExecute,
                &l1_batch_params.base_system_contracts,
            ),
        };

        while let Ok(cmd) = self.commands.recv() {
            match cmd {
                Command::ExecuteTx(tx, resp) => {
                    let result = self.execute_tx(&tx, &mut vm);
                    resp.send(result).unwrap();
                }
                Command::RollbackLastTx(resp) => {
                    self.rollback_last_tx(&mut vm);
                    resp.send(()).unwrap();
                }
                Command::FinishBatch(resp) => {
                    resp.send(self.finish_batch(&mut vm)).unwrap();
                    return;
                }
            }
        }
        // State keeper can exit because of stop signal, so it's OK to exit mid-batch.
        vlog::info!("State keeper exited with an unfinished batch");
    }

    fn execute_tx(&self, tx: &Transaction, vm: &mut VmInstance) -> TxExecutionResult {
        let gas_consumed_before_tx = vm.gas_consumed();

        // Save pre-`execute_next_tx` VM snapshot.
        vm.save_current_vm_as_snapshot();

        // Reject transactions with too big gas limit.
        // They are also rejected on the API level, but
        // we need to secure ourselves in case some tx will somehow get into mempool.
        if tx.gas_limit() > self.max_allowed_tx_gas_limit {
            vlog::warn!(
                "Found tx with too big gas limit in state keeper, hash: {:?}, gas_limit: {}",
                tx.hash(),
                tx.gas_limit()
            );
            return TxExecutionResult {
                tx_result: Err(TxRevertReason::TooBigGasLimit),
                bootloader_dry_run_result: None,
                tx_metrics: None,
                bootloader_dry_run_metrics: None,
                compressed_bytecodes: vec![],
            };
        }

        // Execute the transaction.
        let stage_started_at = Instant::now();
        let tx_result = self.execute_tx_in_vm(tx, vm);
        metrics::histogram!(
            "server.state_keeper.tx_execution_time",
            stage_started_at.elapsed(),
            "stage" => "execution"
        );
        metrics::increment_counter!(
            "server.processed_txs",
            "stage" => "state_keeper"
        );
        metrics::counter!(
            "server.processed_l1_txs",
            tx.is_l1() as u64,
            "stage" => "state_keeper"
        );

        if self.reexecute_each_tx {
            self.reexecute_tx_in_vm(vm, tx, tx_result.clone());
        }

        let mut result = TxExecutionResult::new(tx_result.clone());
        if result.err().is_some() {
            return result;
        }

        let tx_metrics = Self::get_execution_metrics(
            vm,
            Some(tx),
            &tx_result.as_ref().unwrap().0.result,
            gas_consumed_before_tx,
        );
        result.add_tx_metrics(tx_metrics);

        match self.dryrun_block_tip(vm) {
            Ok((exec_result, metrics)) => {
                result.add_bootloader_result(Ok(exec_result));
                result.add_bootloader_metrics(metrics);
            }
            Err(err) => {
                vlog::warn!("VM reverted while executing block tip: {}", err);
                result.add_bootloader_result(Err(err));
            }
        }

        result
    }

    fn rollback_last_tx(&self, vm: &mut VmInstance) {
        let stage_started_at = Instant::now();
        vm.rollback_to_latest_snapshot_popping();
        metrics::histogram!(
            "server.state_keeper.tx_execution_time",
            stage_started_at.elapsed(),
            "stage" => "tx_rollback"
        );
    }

    fn finish_batch(&self, vm: &mut VmInstance) -> VmBlockResult {
        vm.execute_till_block_end(BootloaderJobType::BlockPostprocessing)
    }

    // Err when transaction is rejected.
    // Ok(TxExecutionStatus::Success) when the transaction succeeded
    // Ok(TxExecutionStatus::Failure) when the transaction failed.
    // Note that failed transactions are considered properly processed and are included in blocks
    fn execute_tx_in_vm(
        &self,
        tx: &Transaction,
        vm: &mut VmInstance,
    ) -> Result<(VmTxExecutionResult, Vec<CompressedBytecodeInfo>), TxRevertReason> {
        // Note, that the space where we can put the calldata for compressing transactions
        // is limited and the transactions do not pay for taking it.
        // In order to not let the accounts spam the space of compressed bytecodes with bytecodes
        // that will not be published (e.g. due to out of gas), we use the following scheme:
        // We try to execute the transaction with compressed bytecodes.
        // If it fails and the compressed bytecodes have not been published,
        // it means that there is no sense in pollutting the space of compressed bytecodes,
        // and so we reeexecute the transaction, but without compressions.

        // Saving the snapshot before executing
        vm.save_current_vm_as_snapshot();

        let compressed_bytecodes = if tx.is_l1() || tx.execute.factory_deps.is_none() {
            // For L1 transactions there are no compressed bytecodes
            vec![]
        } else {
            tx.execute
                .factory_deps
                .as_ref()
                .unwrap()
                .iter()
                .filter(|dep| {
                    !vm.state
                        .storage
                        .storage
                        .get_ptr()
                        .borrow_mut()
                        .is_bytecode_known(&hash_bytecode(dep))
                })
                .filter_map(|dep| CompressedBytecodeInfo::from_original(dep.clone()).ok())
                .collect()
        };

        push_transaction_to_bootloader_memory(
            vm,
            tx,
            TxExecutionMode::VerifyExecute,
            Some(compressed_bytecodes.clone()),
        );
        let result_with_compression =
            vm.execute_next_tx(self.validation_computational_gas_limit)?;

        let at_least_one_unpublished = compressed_bytecodes.iter().any(|info| {
            !vm.state
                .storage
                .storage
                .get_ptr()
                .borrow_mut()
                .is_bytecode_known(&hash_bytecode(&info.original))
        });

        if at_least_one_unpublished {
            // Rollbacking and now trying to execute one more time.
            vm.rollback_to_latest_snapshot_popping();
            push_transaction_to_bootloader_memory(
                vm,
                tx,
                TxExecutionMode::VerifyExecute,
                Some(vec![]),
            );

            vm.execute_next_tx(self.validation_computational_gas_limit)
                .map(|val| (val, vec![]))
        } else {
            // Remove the snapshot taken at the start of this function as it is not needed anymore.
            vm.pop_snapshot_no_rollback();

            Ok((result_with_compression, compressed_bytecodes))
        }
    }

    fn reexecute_tx_in_vm(
        &self,
        vm: &mut VmInstance<'_>,
        tx: &Transaction,
        expected_tx_result: Result<
            (VmTxExecutionResult, Vec<CompressedBytecodeInfo>),
            TxRevertReason,
        >,
    ) {
        // Rollback to the pre-`execute_next_tx` VM snapshot.
        // `rollback_to_latest_snapshot` (not `rollback_to_latest_snapshot_popping`) is used here because
        // we will need this snapshot again if seal criteria will result in `ExcludeAndSeal`.
        vm.rollback_to_latest_snapshot();
        let alternative_result = self.execute_tx_in_vm(tx, vm);
        assert_eq!(
            alternative_result,
            expected_tx_result,
            "Failed to reexecute transaction {}",
            tx.hash()
        );
    }

    fn dryrun_block_tip(
        &self,
        vm: &mut VmInstance,
    ) -> Result<(VmPartialExecutionResult, ExecutionMetricsForCriteria), TxRevertReason> {
        let stage_started_at = Instant::now();
        let gas_consumed_before = vm.gas_consumed();

        // Save pre-`execute_till_block_end` VM snapshot.
        vm.save_current_vm_as_snapshot();
        let block_tip_result = vm.execute_block_tip();
        let result = match &block_tip_result.revert_reason {
            None => {
                let metrics =
                    Self::get_execution_metrics(vm, None, &block_tip_result, gas_consumed_before);
                Ok((block_tip_result, metrics))
            }
            Some(TxRevertReason::BootloaderOutOfGas) => Err(TxRevertReason::BootloaderOutOfGas),
            Some(other_reason) => {
                panic!("VM must not revert when finalizing block (except `BootloaderOutOfGas`). Revert reason: {:?}", other_reason);
            }
        };

        // Rollback to the pre-`execute_till_block_end` state.
        vm.rollback_to_latest_snapshot_popping();

        metrics::histogram!(
            "server.state_keeper.tx_execution_time",
            stage_started_at.elapsed(),
            "stage" => "dryrun_block_tip"
        );

        result
    }

    fn get_execution_metrics(
        vm: &VmInstance,
        tx: Option<&Transaction>,
        execution_result: &VmPartialExecutionResult,
        gas_consumed_before: u32,
    ) -> ExecutionMetricsForCriteria {
        let gas_consumed_after = vm.gas_consumed();
        assert!(
            gas_consumed_after >= gas_consumed_before,
            "Invalid consumed gas value, possible underflow. Tx: {:?}",
            tx
        );
        let gas_used = gas_consumed_after - gas_consumed_before;
        let total_factory_deps = tx
            .map(|tx| {
                tx.execute
                    .factory_deps
                    .as_ref()
                    .map_or(0, |deps| deps.len() as u16)
            })
            .unwrap_or(0);

        let execution_metrics = ExecutionMetrics::new(
            &execution_result.logs,
            gas_used as usize,
            total_factory_deps,
            execution_result.contracts_used,
            execution_result.cycles_used,
        );

        let l1_gas = match tx {
            Some(tx) => gas_count_from_tx_and_metrics(tx, &execution_metrics),
            None => gas_count_from_metrics(&execution_metrics),
        };

        ExecutionMetricsForCriteria {
            l1_gas,
            execution_metrics,
        }
    }
}
