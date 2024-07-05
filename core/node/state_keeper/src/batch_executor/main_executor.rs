use std::sync::Arc;

use anyhow::Context as _;
use async_trait::async_trait;
use multivm::{
    interface::{
        ExecutionResult, FinishedL1Batch, Halt, L1BatchEnv, L2BlockEnv, SystemEnv,
        VmExecutionResultAndLogs, VmInterface, VmInterfaceHistoryEnabled,
    },
    tracers::CallTracer,
    vm_latest::HistoryEnabled,
    MultiVMTracer, VmInstance,
};
use once_cell::sync::OnceCell;
use tokio::{
    runtime::Handle,
    sync::{mpsc, watch},
};
use zksync_shared_metrics::{InteractionType, TxStage, APP_METRICS};
use zksync_state::{ReadStorage, ReadStorageFactory, StorageView, WriteStorage};
use zksync_types::{vm_trace::Call, Transaction};
use zksync_utils::bytecode::CompressedBytecodeInfo;

use super::{BatchExecutor, BatchExecutorHandle, Command, TxExecutionResult};
use crate::{
    metrics::{TxExecutionStage, BATCH_TIP_METRICS, EXECUTOR_METRICS, KEEPER_METRICS},
    types::ExecutionMetricsForCriteria,
};

/// The default implementation of [`BatchExecutor`].
/// Creates a "real" batch executor which maintains the VM (as opposed to the test builder which doesn't use the VM).
#[derive(Debug, Clone)]
pub struct MainBatchExecutor {
    save_call_traces: bool,
    /// Whether batch executor would allow transactions with bytecode that cannot be compressed.
    /// For new blocks, bytecode compression is mandatory -- if bytecode compression is not supported,
    /// the transaction will be rejected.
    /// Note that this flag, if set to `true`, is strictly more permissive than if set to `false`. It means
    /// that in cases where the node is expected to process any transactions processed by the sequencer
    /// regardless of its configuration, this flag should be set to `true`.
    optional_bytecode_compression: bool,
}

impl MainBatchExecutor {
    pub fn new(save_call_traces: bool, optional_bytecode_compression: bool) -> Self {
        Self {
            save_call_traces,
            optional_bytecode_compression,
        }
    }
}

#[async_trait]
impl BatchExecutor for MainBatchExecutor {
    async fn init_batch(
        &mut self,
        storage_factory: Arc<dyn ReadStorageFactory>,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Option<BatchExecutorHandle> {
        // Since we process `BatchExecutor` commands one-by-one (the next command is never enqueued
        // until a previous command is processed), capacity 1 is enough for the commands channel.
        let (commands_sender, commands_receiver) = mpsc::channel(1);
        let executor = CommandReceiver {
            save_call_traces: self.save_call_traces,
            optional_bytecode_compression: self.optional_bytecode_compression,
            commands: commands_receiver,
        };

        let stop_receiver = stop_receiver.clone();
        let handle = tokio::task::spawn_blocking(move || {
            if let Some(storage) = Handle::current()
                .block_on(
                    storage_factory.access_storage(&stop_receiver, l1_batch_params.number - 1),
                )
                .context("failed accessing state keeper storage")?
            {
                executor.run(storage, l1_batch_params, system_env);
            } else {
                tracing::info!("Interrupted while trying to access state keeper storage");
            }
            anyhow::Ok(())
        });
        Some(BatchExecutorHandle::from_raw(handle, commands_sender))
    }
}

/// Implementation of the "primary" (non-test) batch executor.
/// Upon launch, it initializes the VM object with provided block context and properties, and keeps invoking the commands
/// sent to it one by one until the batch is finished.
///
/// One `CommandReceiver` can execute exactly one batch, so once the batch is sealed, a new `CommandReceiver` object must
/// be constructed.
#[derive(Debug)]
struct CommandReceiver {
    save_call_traces: bool,
    optional_bytecode_compression: bool,
    commands: mpsc::Receiver<Command>,
}

impl CommandReceiver {
    pub(super) fn run<S: ReadStorage>(
        mut self,
        secondary_storage: S,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
    ) {
        tracing::info!("Starting executing L1 batch #{}", &l1_batch_params.number);

        let storage_view = StorageView::new(secondary_storage).to_rc_ptr();

        let mut vm = VmInstance::new(l1_batch_params, system_env, storage_view.clone());

        while let Some(cmd) = self.commands.blocking_recv() {
            match cmd {
                Command::ExecuteTx(tx, resp) => {
                    let result = self.execute_tx(&tx, &mut vm);
                    if resp.send(result).is_err() {
                        break;
                    }
                }
                Command::RollbackLastTx(resp) => {
                    self.rollback_last_tx(&mut vm);
                    if resp.send(()).is_err() {
                        break;
                    }
                }
                Command::StartNextL2Block(l2_block_env, resp) => {
                    self.start_next_l2_block(l2_block_env, &mut vm);
                    if resp.send(()).is_err() {
                        break;
                    }
                }
                Command::FinishBatch(resp) => {
                    let vm_block_result = self.finish_batch(&mut vm);
                    if resp.send(vm_block_result).is_err() {
                        break;
                    }

                    // `storage_view` cannot be accessed while borrowed by the VM,
                    // so this is the only point at which storage metrics can be obtained
                    let metrics = storage_view.as_ref().borrow_mut().metrics();
                    EXECUTOR_METRICS.batch_storage_interaction_duration[&InteractionType::GetValue]
                        .observe(metrics.time_spent_on_get_value);
                    EXECUTOR_METRICS.batch_storage_interaction_duration[&InteractionType::SetValue]
                        .observe(metrics.time_spent_on_set_value);
                    return;
                }
            }
        }
        // State keeper can exit because of stop signal, so it's OK to exit mid-batch.
        tracing::info!("State keeper exited with an unfinished L1 batch");
    }

    fn execute_tx<S: WriteStorage>(
        &self,
        tx: &Transaction,
        vm: &mut VmInstance<S, HistoryEnabled>,
    ) -> TxExecutionResult {
        // Save pre-`execute_next_tx` VM snapshot.
        vm.make_snapshot();

        // Execute the transaction.
        let latency = KEEPER_METRICS.tx_execution_time[&TxExecutionStage::Execution].start();
        let (tx_result, compressed_bytecodes, call_tracer_result) =
            if self.optional_bytecode_compression {
                self.execute_tx_in_vm_with_optional_compression(tx, vm)
            } else {
                self.execute_tx_in_vm(tx, vm)
            };
        let latency = latency.observe();
        APP_METRICS.processed_txs[&TxStage::StateKeeper].inc();
        APP_METRICS.processed_l1_txs[&TxStage::StateKeeper].inc_by(tx.is_l1().into());

        if let ExecutionResult::Halt { reason } = tx_result.result {
            return match reason {
                Halt::BootloaderOutOfGas => TxExecutionResult::BootloaderOutOfGasForTx,
                _ => TxExecutionResult::RejectedByVm { reason },
            };
        }

        let tx_metrics = ExecutionMetricsForCriteria::new(Some(tx), &tx_result);
        tracing::info!("Transaction {tx:?} executed in {latency:?} with {tx_metrics:?}"); // FIXME: remove
        let gas_remaining = vm.gas_remaining();

        TxExecutionResult::Success {
            tx_result: Box::new(tx_result),
            tx_metrics: Box::new(tx_metrics),
            compressed_bytecodes,
            call_tracer_result,
            gas_remaining,
        }
    }

    fn rollback_last_tx<S: WriteStorage>(&self, vm: &mut VmInstance<S, HistoryEnabled>) {
        let latency = KEEPER_METRICS.tx_execution_time[&TxExecutionStage::TxRollback].start();
        vm.rollback_to_the_latest_snapshot();
        latency.observe();
    }

    fn start_next_l2_block<S: WriteStorage>(
        &self,
        l2_block_env: L2BlockEnv,
        vm: &mut VmInstance<S, HistoryEnabled>,
    ) {
        vm.start_new_l2_block(l2_block_env);
    }

    fn finish_batch<S: WriteStorage>(
        &self,
        vm: &mut VmInstance<S, HistoryEnabled>,
    ) -> FinishedL1Batch {
        // The vm execution was paused right after the last transaction was executed.
        // There is some post-processing work that the VM needs to do before the block is fully processed.
        let result = vm.finish_batch();
        if result.block_tip_execution_result.result.is_failed() {
            panic!(
                "VM must not fail when finalizing block: {:#?}",
                result.block_tip_execution_result.result
            );
        }

        BATCH_TIP_METRICS.observe(&result.block_tip_execution_result);
        result
    }

    /// Attempts to execute transaction with or without bytecode compression.
    /// If compression fails, the transaction will be re-executed without compression.
    fn execute_tx_in_vm_with_optional_compression<S: WriteStorage>(
        &self,
        tx: &Transaction,
        vm: &mut VmInstance<S, HistoryEnabled>,
    ) -> (
        VmExecutionResultAndLogs,
        Vec<CompressedBytecodeInfo>,
        Vec<Call>,
    ) {
        // Note, that the space where we can put the calldata for compressing transactions
        // is limited and the transactions do not pay for taking it.
        // In order to not let the accounts spam the space of compressed bytecodes with bytecodes
        // that will not be published (e.g. due to out of gas), we use the following scheme:
        // We try to execute the transaction with compressed bytecodes.
        // If it fails and the compressed bytecodes have not been published,
        // it means that there is no sense in polluting the space of compressed bytecodes,
        // and so we re-execute the transaction, but without compression.

        // Saving the snapshot before executing
        vm.make_snapshot();

        let call_tracer_result = Arc::new(OnceCell::default());
        let tracer = if self.save_call_traces {
            vec![CallTracer::new(call_tracer_result.clone()).into_tracer_pointer()]
        } else {
            vec![]
        };

        if let (Ok(()), result) =
            vm.inspect_transaction_with_bytecode_compression(tracer.into(), tx.clone(), true)
        {
            let compressed_bytecodes = vm.get_last_tx_compressed_bytecodes();
            vm.pop_snapshot_no_rollback();

            let trace = Arc::try_unwrap(call_tracer_result)
                .unwrap()
                .take()
                .unwrap_or_default();
            return (result, compressed_bytecodes, trace);
        }
        vm.rollback_to_the_latest_snapshot();

        let call_tracer_result = Arc::new(OnceCell::default());
        let tracer = if self.save_call_traces {
            vec![CallTracer::new(call_tracer_result.clone()).into_tracer_pointer()]
        } else {
            vec![]
        };

        let result =
            vm.inspect_transaction_with_bytecode_compression(tracer.into(), tx.clone(), false);
        result
            .0
            .expect("Compression can't fail if we don't apply it");
        let compressed_bytecodes = vm.get_last_tx_compressed_bytecodes();

        // TODO implement tracer manager which will be responsible
        // for collecting result from all tracers and save it to the database
        let trace = Arc::try_unwrap(call_tracer_result)
            .unwrap()
            .take()
            .unwrap_or_default();
        (result.1, compressed_bytecodes, trace)
    }

    /// Attempts to execute transaction with mandatory bytecode compression.
    /// If bytecode compression fails, the transaction will be rejected.
    fn execute_tx_in_vm<S: WriteStorage>(
        &self,
        tx: &Transaction,
        vm: &mut VmInstance<S, HistoryEnabled>,
    ) -> (
        VmExecutionResultAndLogs,
        Vec<CompressedBytecodeInfo>,
        Vec<Call>,
    ) {
        let call_tracer_result = Arc::new(OnceCell::default());
        let tracer = if self.save_call_traces {
            vec![CallTracer::new(call_tracer_result.clone()).into_tracer_pointer()]
        } else {
            vec![]
        };

        let (published_bytecodes, mut result) =
            vm.inspect_transaction_with_bytecode_compression(tracer.into(), tx.clone(), true);
        if published_bytecodes.is_ok() {
            let compressed_bytecodes = vm.get_last_tx_compressed_bytecodes();

            let trace = Arc::try_unwrap(call_tracer_result)
                .unwrap()
                .take()
                .unwrap_or_default();
            (result, compressed_bytecodes, trace)
        } else {
            // Transaction failed to publish bytecodes, we reject it so initiator doesn't pay fee.
            result.result = ExecutionResult::Halt {
                reason: Halt::FailedToPublishCompressedBytecodes,
            };
            (result, Default::default(), Default::default())
        }
    }
}
