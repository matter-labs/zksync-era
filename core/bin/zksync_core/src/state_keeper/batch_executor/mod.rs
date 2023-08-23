use async_trait::async_trait;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use std::{collections::HashSet, fmt, time::Instant};

use multivm::{
    init_vm, init_vm_with_gas_limit, BlockProperties, OracleTools, VmInstance, VmVersion,
};
use vm::{
    vm::{VmPartialExecutionResult, VmTxExecutionResult},
    vm_with_bootloader::{BootloaderJobType, TxExecutionMode},
    TxRevertReason, VmBlockResult,
};
use zksync_dal::ConnectionPool;
use zksync_state::{RocksdbStorage, StorageView};
use zksync_types::{tx::ExecutionMetrics, L1BatchNumber, Transaction, U256};
use zksync_utils::bytecode::{hash_bytecode, CompressedBytecodeInfo};

#[cfg(test)]
mod tests;

use crate::{
    gas_tracker::{gas_count_from_metrics, gas_count_from_tx_and_metrics},
    state_keeper::{io::L1BatchParams, types::ExecutionMetricsForCriteria},
};

/// Representation of a transaction executed in the virtual machine.
#[derive(Debug, Clone)]
pub(crate) enum TxExecutionResult {
    /// Successful execution of the tx and the block tip dry run.
    Success {
        tx_result: Box<VmTxExecutionResult>,
        tx_metrics: ExecutionMetricsForCriteria,
        bootloader_dry_run_metrics: ExecutionMetricsForCriteria,
        bootloader_dry_run_result: Box<VmPartialExecutionResult>,
        compressed_bytecodes: Vec<CompressedBytecodeInfo>,
    },
    /// The VM rejected the tx for some reason.
    RejectedByVm { rejection_reason: TxRevertReason },
    /// Bootloader gas limit is not enough to execute the tx.
    BootloaderOutOfGasForTx,
    /// Bootloader gas limit is enough to run the tx but not enough to execute block tip.
    BootloaderOutOfGasForBlockTip,
}

impl TxExecutionResult {
    /// Returns a revert reason if either transaction was rejected or bootloader ran out of gas.
    pub(super) fn err(&self) -> Option<&TxRevertReason> {
        match self {
            Self::Success { .. } => None,
            Self::RejectedByVm { rejection_reason } => Some(rejection_reason),
            Self::BootloaderOutOfGasForTx | Self::BootloaderOutOfGasForBlockTip { .. } => {
                Some(&TxRevertReason::BootloaderOutOfGas)
            }
        }
    }
}

/// Configuration for the MultiVM.
/// Currently, represents an ordered sequence of (min_batch_number, vm_version) entries,
/// which will be scanned by the MultiVM on each batch.
#[derive(Debug, Clone)]
pub struct MultiVMConfig {
    versions: Vec<(L1BatchNumber, VmVersion)>,
}

impl MultiVMConfig {
    /// Creates a new MultiVM config from the provided sequence of (min_batch_number, vm_version) entries.
    ///
    /// ## Panics
    ///
    /// Panics if the provided sequence is not ordered by the batch number, if it's empty or if the first entry
    /// doesn't correspond to the batch #1.
    pub fn new(versions: Vec<(L1BatchNumber, VmVersion)>) -> Self {
        // Must-haves: config is not empty, we start from the first batch, config is ordered.
        assert!(!versions.is_empty());
        assert_eq!(versions[0].0 .0, 1);
        assert!(versions.windows(2).all(|w| w[0].0 < w[1].0));

        Self { versions }
    }

    /// Finds the appropriate VM version for the provided batch number.
    pub fn version_for(&self, batch_number: L1BatchNumber) -> VmVersion {
        debug_assert!(
            batch_number != L1BatchNumber(0),
            "Genesis block doesn't need to be actually executed"
        );
        // Find the latest version which is not greater than the provided batch number.
        let (_, version) = *self
            .versions
            .iter()
            .rev()
            .find(|(version_start, _)| batch_number >= *version_start)
            .expect("At least one version must match");
        version
    }

    /// Returns the config for mainnet.
    /// This method is WIP, and returned config is not guaranteed to be full or correct.
    pub fn mainnet_config_wip() -> Self {
        Self::new(vec![
            (L1BatchNumber(1), VmVersion::M5WithoutRefunds),
            (L1BatchNumber(292), VmVersion::M5WithRefunds),
            (L1BatchNumber(360), VmVersion::M6Initial),
            (L1BatchNumber(390), VmVersion::M6BugWithCompressionFixed),
            (L1BatchNumber(49508), VmVersion::Vm1_3_2),
        ])
    }

    /// Returns the config for testnet.
    /// This method is WIP, and returned config is not guaranteed to be full or correct.
    pub fn testnet_config_wip() -> Self {
        Self::new(vec![(L1BatchNumber(1), VmVersion::M5WithoutRefunds)])
    }
}

/// An abstraction that allows us to create different kinds of batch executors.
/// The only requirement is to return a [`BatchExecutorHandle`], which does its work
/// by communicating with the externally initialized thread.
#[async_trait]
pub trait L1BatchExecutorBuilder: 'static + Send + Sync + fmt::Debug {
    async fn init_batch(&self, l1_batch_params: L1BatchParams) -> BatchExecutorHandle;
}

/// The default implementation of [`L1BatchExecutorBuilder`].
/// Creates a "real" batch executor which maintains the VM (as opposed to the test builder which doesn't use the VM).
#[derive(Debug, Clone)]
pub struct MainBatchExecutorBuilder {
    state_keeper_db_path: String,
    pool: ConnectionPool,
    save_call_traces: bool,
    max_allowed_tx_gas_limit: U256,
    validation_computational_gas_limit: u32,
    multivm_config: Option<MultiVMConfig>,
}

impl MainBatchExecutorBuilder {
    pub fn new(
        state_keeper_db_path: String,
        pool: ConnectionPool,
        max_allowed_tx_gas_limit: U256,
        save_call_traces: bool,
        validation_computational_gas_limit: u32,
        multivm_config: Option<MultiVMConfig>,
    ) -> Self {
        Self {
            state_keeper_db_path,
            pool,
            save_call_traces,
            max_allowed_tx_gas_limit,
            validation_computational_gas_limit,
            multivm_config,
        }
    }
}

#[async_trait]
impl L1BatchExecutorBuilder for MainBatchExecutorBuilder {
    async fn init_batch(&self, l1_batch_params: L1BatchParams) -> BatchExecutorHandle {
        let mut secondary_storage = RocksdbStorage::new(self.state_keeper_db_path.as_ref());
        let mut conn = self.pool.access_storage_tagged("state_keeper").await;
        secondary_storage.update_from_postgres(&mut conn).await;
        drop(conn);

        let batch_number = l1_batch_params
            .context_mode
            .inner_block_context()
            .context
            .block_number;
        let vm_version = self
            .multivm_config
            .as_ref()
            .map(|config| config.version_for(L1BatchNumber(batch_number)))
            .unwrap_or(VmVersion::latest());

        vlog::info!(
            "Secondary storage for batch {batch_number} initialized, size is {}",
            secondary_storage.estimated_map_size()
        );
        metrics::gauge!(
            "server.state_keeper.storage_map_size",
            secondary_storage.estimated_map_size() as f64,
        );
        BatchExecutorHandle::new(
            vm_version,
            self.save_call_traces,
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
    handle: JoinHandle<()>,
    commands: mpsc::Sender<Command>,
}

impl BatchExecutorHandle {
    pub(super) fn new(
        vm_version: VmVersion,
        save_call_traces: bool,
        max_allowed_tx_gas_limit: U256,
        validation_computational_gas_limit: u32,
        secondary_storage: RocksdbStorage,
        l1_batch_params: L1BatchParams,
        vm_gas_limit: Option<u32>,
    ) -> Self {
        // Since we process `BatchExecutor` commands one-by-one (the next command is never enqueued
        // until a previous command is processed), capacity 1 is enough for the commands channel.
        let (commands_sender, commands_receiver) = mpsc::channel(1);
        let executor = BatchExecutor {
            vm_version,
            save_call_traces,
            max_allowed_tx_gas_limit,
            validation_computational_gas_limit,
            commands: commands_receiver,
            vm_gas_limit,
        };

        let handle =
            tokio::task::spawn_blocking(move || executor.run(secondary_storage, l1_batch_params));
        Self {
            handle,
            commands: commands_sender,
        }
    }

    /// Creates a batch executor handle from the provided sender and thread join handle.
    /// Can be used to inject an alternative batch executor implementation.
    #[cfg(test)]
    pub(super) fn from_raw(handle: JoinHandle<()>, commands: mpsc::Sender<Command>) -> Self {
        Self { handle, commands }
    }

    pub(super) async fn execute_tx(&self, tx: Transaction) -> TxExecutionResult {
        let tx_gas_limit = tx.gas_limit().as_u32();

        let (response_sender, response_receiver) = oneshot::channel();
        self.commands
            .send(Command::ExecuteTx(Box::new(tx), response_sender))
            .await
            .unwrap();

        let start = Instant::now();
        let res = response_receiver.await.unwrap();
        let elapsed = start.elapsed();

        metrics::histogram!("state_keeper.batch_executor.command_response_time", elapsed, "command" => "execute_tx");

        if let TxExecutionResult::Success { tx_metrics, .. } = res {
            metrics::histogram!(
                "state_keeper.computational_gas_per_nanosecond",
                tx_metrics.execution_metrics.computational_gas_used as f64
                    / elapsed.as_nanos() as f64
            );
        } else {
            // The amount of computational gas paid for failed transactions is hard to get
            // but comparing to the gas limit makes sense, since we can burn all gas
            // if some kind of failure is a DDoS vector otherwise.
            metrics::histogram!(
                "state_keeper.failed_tx_gas_limit_per_nanosecond",
                tx_gas_limit as f64 / elapsed.as_nanos() as f64
            );
        }

        res
    }

    pub(super) async fn rollback_last_tx(&self) {
        // While we don't get anything from the channel, it's useful to have it as a confirmation that the operation
        // indeed has been processed.
        let (response_sender, response_receiver) = oneshot::channel();
        self.commands
            .send(Command::RollbackLastTx(response_sender))
            .await
            .unwrap();
        let start = Instant::now();
        response_receiver.await.unwrap();
        metrics::histogram!("state_keeper.batch_executor.command_response_time", start.elapsed(), "command" => "rollback_last_tx");
    }

    pub(super) async fn finish_batch(self) -> VmBlockResult {
        let (response_sender, response_receiver) = oneshot::channel();
        self.commands
            .send(Command::FinishBatch(response_sender))
            .await
            .unwrap();
        let start = Instant::now();
        let resp = response_receiver.await.unwrap();
        self.handle.await.unwrap();
        metrics::histogram!("state_keeper.batch_executor.command_response_time", start.elapsed(), "command" => "finish_batch");
        resp
    }
}

#[derive(Debug)]
pub(super) enum Command {
    ExecuteTx(Box<Transaction>, oneshot::Sender<TxExecutionResult>),
    RollbackLastTx(oneshot::Sender<()>),
    FinishBatch(oneshot::Sender<VmBlockResult>),
}

/// Implementation of the "primary" (non-test) batch executor.
/// Upon launch, it initializes the VM object with provided block context and properties, and keeps applying
/// transactions until the batch is sealed.
///
/// One `BatchExecutor` can execute exactly one batch, so once the batch is sealed, a new `BatchExecutor` object must
/// be constructed.
#[derive(Debug)]
pub(super) struct BatchExecutor {
    vm_version: VmVersion,
    save_call_traces: bool,
    max_allowed_tx_gas_limit: U256,
    validation_computational_gas_limit: u32,
    commands: mpsc::Receiver<Command>,
    vm_gas_limit: Option<u32>,
}

impl BatchExecutor {
    pub(super) fn run(mut self, secondary_storage: RocksdbStorage, l1_batch_params: L1BatchParams) {
        vlog::info!(
            "Starting executing batch #{}",
            l1_batch_params
                .context_mode
                .inner_block_context()
                .context
                .block_number
        );

        let mut storage_view = StorageView::new(&secondary_storage);
        let mut oracle_tools = OracleTools::new(self.vm_version, &mut storage_view);
        let block_properties = BlockProperties::new(
            self.vm_version,
            l1_batch_params.properties.default_aa_code_hash,
        );
        let mut vm = match self.vm_gas_limit {
            Some(vm_gas_limit) => init_vm_with_gas_limit(
                self.vm_version,
                &mut oracle_tools,
                l1_batch_params.context_mode,
                &block_properties,
                TxExecutionMode::VerifyExecute,
                &l1_batch_params.base_system_contracts,
                vm_gas_limit,
            ),
            None => init_vm(
                self.vm_version,
                &mut oracle_tools,
                l1_batch_params.context_mode,
                &block_properties,
                TxExecutionMode::VerifyExecute,
                &l1_batch_params.base_system_contracts,
            ),
        };

        while let Some(cmd) = self.commands.blocking_recv() {
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

                    // storage_view cannot be accessed while borrowed by the VM,
                    // so this is the only point at which storage metrics can be obtained
                    let metrics = storage_view.metrics();
                    metrics::histogram!(
                        "state_keeper.batch_storage_interaction_duration",
                        metrics.time_spent_on_get_value,
                        "interaction" => "get_value"
                    );
                    metrics::histogram!(
                        "state_keeper.batch_storage_interaction_duration",
                        metrics.time_spent_on_set_value,
                        "interaction" => "set_value"
                    );

                    return;
                }
            }
        }
        // State keeper can exit because of stop signal, so it's OK to exit mid-batch.
        vlog::info!("State keeper exited with an unfinished batch");
    }

    fn execute_tx(&self, tx: &Transaction, vm: &mut VmInstance<'_>) -> TxExecutionResult {
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
            return TxExecutionResult::RejectedByVm {
                rejection_reason: TxRevertReason::TooBigGasLimit,
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

        let (exec_result, compressed_bytecodes) = match tx_result {
            Err(TxRevertReason::BootloaderOutOfGas) => {
                return TxExecutionResult::BootloaderOutOfGasForTx
            }
            Err(rejection_reason) => return TxExecutionResult::RejectedByVm { rejection_reason },
            Ok((exec_result, compressed_bytecodes)) => (exec_result, compressed_bytecodes),
        };

        let tx_metrics =
            Self::get_execution_metrics(vm, Some(tx), &exec_result.result, gas_consumed_before_tx);

        match self.dryrun_block_tip(vm) {
            Ok((bootloader_dry_run_result, bootloader_dry_run_metrics)) => {
                TxExecutionResult::Success {
                    tx_result: Box::new(exec_result),
                    tx_metrics,
                    bootloader_dry_run_metrics,
                    bootloader_dry_run_result: Box::new(bootloader_dry_run_result),
                    compressed_bytecodes,
                }
            }
            Err(err) => {
                vlog::warn!("VM reverted while executing block tip: {}", err);
                TxExecutionResult::BootloaderOutOfGasForBlockTip
            }
        }
    }

    fn rollback_last_tx(&self, vm: &mut VmInstance<'_>) {
        let stage_started_at = Instant::now();
        vm.rollback_to_snapshot_popping();
        metrics::histogram!(
            "server.state_keeper.tx_execution_time",
            stage_started_at.elapsed(),
            "stage" => "tx_rollback"
        );
    }

    fn finish_batch(&self, vm: &mut VmInstance<'_>) -> VmBlockResult {
        vm.execute_till_block_end(BootloaderJobType::BlockPostprocessing)
    }

    // Err when transaction is rejected.
    // Ok(TxExecutionStatus::Success) when the transaction succeeded
    // Ok(TxExecutionStatus::Failure) when the transaction failed.
    // Note that failed transactions are considered properly processed and are included in blocks
    fn execute_tx_in_vm(
        &self,
        tx: &Transaction,
        vm: &mut VmInstance<'_>,
    ) -> Result<(VmTxExecutionResult, Vec<CompressedBytecodeInfo>), TxRevertReason> {
        // Note, that the space where we can put the calldata for compressing transactions
        // is limited and the transactions do not pay for taking it.
        // In order to not let the accounts spam the space of compressed bytecodes with bytecodes
        // that will not be published (e.g. due to out of gas), we use the following scheme:
        // We try to execute the transaction with compressed bytecodes.
        // If it fails and the compressed bytecodes have not been published,
        // it means that there is no sense in pollutting the space of compressed bytecodes,
        // and so we reexecute the transaction, but without compressions.

        // Saving the snapshot before executing
        vm.save_current_vm_as_snapshot();

        let compressed_bytecodes = if tx.is_l1() {
            // For L1 transactions there are no compressed bytecodes
            vec![]
        } else {
            // Deduplicate and filter factory deps preserving original order.
            let deps = tx.execute.factory_deps.as_deref().unwrap_or_default();
            let mut deps_hashes = HashSet::with_capacity(deps.len());
            let filtered_deps = deps.iter().filter_map(|bytecode| {
                let bytecode_hash = hash_bytecode(bytecode);
                let is_known =
                    !deps_hashes.insert(bytecode_hash) || vm.is_bytecode_known(&bytecode_hash);
                if is_known {
                    None
                } else {
                    CompressedBytecodeInfo::from_original(bytecode.clone()).ok()
                }
            });
            filtered_deps.collect()
        };

        vm.push_transaction_to_bootloader_memory(
            tx,
            TxExecutionMode::VerifyExecute,
            Some(compressed_bytecodes.clone()),
        );
        let result_with_compression = vm.execute_next_tx(
            self.validation_computational_gas_limit,
            self.save_call_traces,
        )?;

        let at_least_one_unpublished = {
            compressed_bytecodes
                .iter()
                .any(|info| !vm.is_bytecode_known(&hash_bytecode(&info.original)))
        };

        if at_least_one_unpublished {
            // Rolling back and trying to execute one more time.
            vm.rollback_to_snapshot_popping();
            vm.push_transaction_to_bootloader_memory(
                tx,
                TxExecutionMode::VerifyExecute,
                Some(vec![]),
            );

            vm.execute_next_tx(
                self.validation_computational_gas_limit,
                self.save_call_traces,
            )
            .map(|val| (val, vec![]))
        } else {
            // Remove the snapshot taken at the start of this function as it is not needed anymore.
            vm.pop_snapshot_no_rollback();
            Ok((result_with_compression, compressed_bytecodes))
        }
    }

    fn dryrun_block_tip(
        &self,
        vm: &mut VmInstance<'_>,
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
        vm.rollback_to_snapshot_popping();

        metrics::histogram!(
            "server.state_keeper.tx_execution_time",
            stage_started_at.elapsed(),
            "stage" => "dryrun_block_tip"
        );

        result
    }

    fn get_execution_metrics(
        vm: &VmInstance<'_>,
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
            execution_result.computational_gas_used,
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
