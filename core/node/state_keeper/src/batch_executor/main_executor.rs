use std::{fmt, sync::Arc};

use anyhow::Context as _;
use once_cell::sync::OnceCell;
use tokio::sync::mpsc;
use zksync_multivm::{
    dump::{VmDump, VmDumpHandler},
    interface::{
        storage::{ReadStorage, StorageView},
        Call, CompressedBytecodeInfo, ExecutionResult, FinishedL1Batch, Halt, L1BatchEnv,
        L2BlockEnv, SystemEnv, VmExecutionResultAndLogs, VmInterface, VmInterfaceHistoryEnabled,
    },
    tracers::CallTracer,
    vm_latest::HistoryEnabled,
    MultiVMTracer, VmInstance,
};
use zksync_shared_metrics::{InteractionType, TxStage, APP_METRICS};
use zksync_types::{vm::FastVmMode, Transaction};

use super::{BatchExecutor, BatchExecutorHandle, Command, TxExecutionResult};
use crate::{
    metrics::{TxExecutionStage, BATCH_TIP_METRICS, EXECUTOR_METRICS, KEEPER_METRICS},
    types::ExecutionMetricsForCriteria,
};

/// The default implementation of [`BatchExecutor`].
/// Creates a "real" batch executor which maintains the VM (as opposed to the test builder which doesn't use the VM).
#[derive(Clone)]
pub struct MainBatchExecutor {
    save_call_traces: bool,
    /// Whether batch executor would allow transactions with bytecode that cannot be compressed.
    /// For new blocks, bytecode compression is mandatory -- if bytecode compression is not supported,
    /// the transaction will be rejected.
    /// Note that this flag, if set to `true`, is strictly more permissive than if set to `false`. It means
    /// that in cases where the node is expected to process any transactions processed by the sequencer
    /// regardless of its configuration, this flag should be set to `true`.
    optional_bytecode_compression: bool,
    fast_vm_mode: FastVmMode,
    dump_handler: Option<VmDumpHandler>,
}

impl fmt::Debug for MainBatchExecutor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MainBatchExecutor")
            .field("save_call_traces", &self.save_call_traces)
            .field(
                "optional_bytecode_compression",
                &self.optional_bytecode_compression,
            )
            .field("fast_vm_mode", &self.fast_vm_mode)
            .finish_non_exhaustive()
    }
}

impl MainBatchExecutor {
    pub fn new(save_call_traces: bool, optional_bytecode_compression: bool) -> Self {
        Self {
            save_call_traces,
            optional_bytecode_compression,
            fast_vm_mode: FastVmMode::Old,
            dump_handler: None,
        }
    }

    pub fn set_fast_vm_mode(&mut self, fast_vm_mode: FastVmMode) {
        if !matches!(fast_vm_mode, FastVmMode::Old) {
            tracing::warn!(
                "Running new VM with mode {fast_vm_mode:?}; this can lead to incorrect node behavior"
            );
        }
        self.fast_vm_mode = fast_vm_mode;
    }

    pub fn set_dump_handler(&mut self, handler: Arc<dyn Fn(VmDump) + Send + Sync>) {
        tracing::info!("Set VM dumps handler");
        self.dump_handler = Some(handler);
    }
}

impl<S: ReadStorage + Send + 'static> BatchExecutor<S> for MainBatchExecutor {
    fn init_batch(
        &mut self,
        storage: S,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
    ) -> BatchExecutorHandle {
        // Since we process `BatchExecutor` commands one-by-one (the next command is never enqueued
        // until a previous command is processed), capacity 1 is enough for the commands channel.
        let (commands_sender, commands_receiver) = mpsc::channel(1);
        let executor = CommandReceiver {
            save_call_traces: self.save_call_traces,
            optional_bytecode_compression: self.optional_bytecode_compression,
            fast_vm_mode: self.fast_vm_mode,
            dump_handler: self.dump_handler.clone(),
            commands: commands_receiver,
        };

        let handle =
            tokio::task::spawn_blocking(move || executor.run(storage, l1_batch_params, system_env));
        BatchExecutorHandle::from_raw(handle, commands_sender)
    }
}

#[derive(Debug)]
struct TransactionOutput {
    tx_result: VmExecutionResultAndLogs,
    compressed_bytecodes: Vec<CompressedBytecodeInfo>,
    calls: Vec<Call>,
}

/// Implementation of the "primary" (non-test) batch executor.
/// Upon launch, it initializes the VM object with provided block context and properties, and keeps invoking the commands
/// sent to it one by one until the batch is finished.
///
/// One `CommandReceiver` can execute exactly one batch, so once the batch is sealed, a new `CommandReceiver` object must
/// be constructed.
struct CommandReceiver {
    save_call_traces: bool,
    optional_bytecode_compression: bool,
    fast_vm_mode: FastVmMode,
    dump_handler: Option<VmDumpHandler>,
    commands: mpsc::Receiver<Command>,
}

impl fmt::Debug for CommandReceiver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CommandReceiver")
            .field("save_call_traces", &self.save_call_traces)
            .field(
                "optional_bytecode_compression",
                &self.optional_bytecode_compression,
            )
            .field("fast_vm_mode", &self.fast_vm_mode)
            .finish_non_exhaustive()
    }
}

impl CommandReceiver {
    pub(super) fn run<S: ReadStorage>(
        mut self,
        storage: S,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
    ) -> anyhow::Result<()> {
        tracing::info!("Starting executing L1 batch #{}", &l1_batch_params.number);

        let storage_view = StorageView::new(storage).to_rc_ptr();
        let mut vm = VmInstance::maybe_fast(
            l1_batch_params,
            system_env,
            storage_view.clone(),
            self.fast_vm_mode,
        );

        if let VmInstance::ShadowedVmFast(vm) = &mut vm {
            if let Some(handler) = self.dump_handler.take() {
                vm.set_dump_handler(handler);
            }
        }

        while let Some(cmd) = self.commands.blocking_recv() {
            match cmd {
                Command::ExecuteTx(tx, resp) => {
                    let result = self
                        .execute_tx(&tx, &mut vm)
                        .with_context(|| format!("fatal error executing transaction {tx:?}"))?;
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
                    let vm_block_result = self.finish_batch(&mut vm)?;
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
                    return Ok(());
                }
                Command::FinishBatchWithCache(resp) => {
                    let vm_block_result = self.finish_batch(&mut vm)?;
                    let cache = (*storage_view).borrow().cache();
                    if resp.send((vm_block_result, cache)).is_err() {
                        break;
                    }
                    return Ok(());
                }
            }
        }
        // State keeper can exit because of stop signal, so it's OK to exit mid-batch.
        tracing::info!("State keeper exited with an unfinished L1 batch");
        Ok(())
    }

    fn execute_tx<S: ReadStorage>(
        &self,
        tx: &Transaction,
        vm: &mut VmInstance<S, HistoryEnabled>,
    ) -> anyhow::Result<TxExecutionResult> {
        // Executing a next transaction means that a previous transaction was either rolled back (in which case its snapshot
        // was already removed), or that we build on top of it (in which case, it can be removed now).
        vm.pop_snapshot_no_rollback();
        // Save pre-execution VM snapshot.
        vm.make_snapshot();

        // Execute the transaction.
        let latency = KEEPER_METRICS.tx_execution_time[&TxExecutionStage::Execution].start();
        let output = if self.optional_bytecode_compression {
            self.execute_tx_in_vm_with_optional_compression(tx, vm)?
        } else {
            self.execute_tx_in_vm(tx, vm)?
        };
        latency.observe();
        APP_METRICS.processed_txs[&TxStage::StateKeeper].inc();
        APP_METRICS.processed_l1_txs[&TxStage::StateKeeper].inc_by(tx.is_l1().into());

        let TransactionOutput {
            tx_result,
            compressed_bytecodes,
            calls,
        } = output;

        if let ExecutionResult::Halt { reason } = tx_result.result {
            return Ok(match reason {
                Halt::BootloaderOutOfGas => TxExecutionResult::BootloaderOutOfGasForTx,
                _ => TxExecutionResult::RejectedByVm { reason },
            });
        }

        let tx_metrics = ExecutionMetricsForCriteria::new(Some(tx), &tx_result);
        let gas_remaining = vm.gas_remaining();

        Ok(TxExecutionResult::Success {
            tx_result: Box::new(tx_result),
            tx_metrics: Box::new(tx_metrics),
            compressed_bytecodes,
            call_tracer_result: calls,
            gas_remaining,
        })
    }

    fn rollback_last_tx<S: ReadStorage>(&self, vm: &mut VmInstance<S, HistoryEnabled>) {
        let latency = KEEPER_METRICS.tx_execution_time[&TxExecutionStage::TxRollback].start();
        vm.rollback_to_the_latest_snapshot();
        latency.observe();
    }

    fn start_next_l2_block<S: ReadStorage>(
        &self,
        l2_block_env: L2BlockEnv,
        vm: &mut VmInstance<S, HistoryEnabled>,
    ) {
        vm.start_new_l2_block(l2_block_env);
    }

    fn finish_batch<S: ReadStorage>(
        &self,
        vm: &mut VmInstance<S, HistoryEnabled>,
    ) -> anyhow::Result<FinishedL1Batch> {
        // The vm execution was paused right after the last transaction was executed.
        // There is some post-processing work that the VM needs to do before the block is fully processed.
        let result = vm.finish_batch();
        anyhow::ensure!(
            !result.block_tip_execution_result.result.is_failed(),
            "VM must not fail when finalizing block: {:#?}",
            result.block_tip_execution_result.result
        );

        BATCH_TIP_METRICS.observe(&result.block_tip_execution_result);
        Ok(result)
    }

    /// Attempts to execute transaction with or without bytecode compression.
    /// If compression fails, the transaction will be re-executed without compression.
    fn execute_tx_in_vm_with_optional_compression<S: ReadStorage>(
        &self,
        tx: &Transaction,
        vm: &mut VmInstance<S, HistoryEnabled>,
    ) -> anyhow::Result<TransactionOutput> {
        // Note, that the space where we can put the calldata for compressing transactions
        // is limited and the transactions do not pay for taking it.
        // In order to not let the accounts spam the space of compressed bytecodes with bytecodes
        // that will not be published (e.g. due to out of gas), we use the following scheme:
        // We try to execute the transaction with compressed bytecodes.
        // If it fails and the compressed bytecodes have not been published,
        // it means that there is no sense in polluting the space of compressed bytecodes,
        // and so we re-execute the transaction, but without compression.

        let call_tracer_result = Arc::new(OnceCell::default());
        let tracer = if self.save_call_traces {
            vec![CallTracer::new(call_tracer_result.clone()).into_tracer_pointer()]
        } else {
            vec![]
        };

        if let (Ok(()), tx_result) =
            vm.inspect_transaction_with_bytecode_compression(tracer.into(), tx.clone(), true)
        {
            let compressed_bytecodes = vm.get_last_tx_compressed_bytecodes();

            let calls = Arc::try_unwrap(call_tracer_result)
                .map_err(|_| anyhow::anyhow!("failed extracting call traces"))?
                .take()
                .unwrap_or_default();
            return Ok(TransactionOutput {
                tx_result,
                compressed_bytecodes,
                calls,
            });
        }

        // Roll back to the snapshot just before the transaction execution taken in `Self::execute_tx()`
        // and create a snapshot at the same VM state again.
        vm.rollback_to_the_latest_snapshot();
        vm.make_snapshot();

        let call_tracer_result = Arc::new(OnceCell::default());
        let tracer = if self.save_call_traces {
            vec![CallTracer::new(call_tracer_result.clone()).into_tracer_pointer()]
        } else {
            vec![]
        };

        let (compression_result, tx_result) =
            vm.inspect_transaction_with_bytecode_compression(tracer.into(), tx.clone(), false);
        compression_result.context("compression failed when it wasn't applied")?;
        let compressed_bytecodes = vm.get_last_tx_compressed_bytecodes();

        // TODO implement tracer manager which will be responsible
        //   for collecting result from all tracers and save it to the database
        let calls = Arc::try_unwrap(call_tracer_result)
            .map_err(|_| anyhow::anyhow!("failed extracting call traces"))?
            .take()
            .unwrap_or_default();
        Ok(TransactionOutput {
            tx_result,
            compressed_bytecodes,
            calls,
        })
    }

    /// Attempts to execute transaction with mandatory bytecode compression.
    /// If bytecode compression fails, the transaction will be rejected.
    fn execute_tx_in_vm<S: ReadStorage>(
        &self,
        tx: &Transaction,
        vm: &mut VmInstance<S, HistoryEnabled>,
    ) -> anyhow::Result<TransactionOutput> {
        let call_tracer_result = Arc::new(OnceCell::default());
        let tracer = if self.save_call_traces {
            vec![CallTracer::new(call_tracer_result.clone()).into_tracer_pointer()]
        } else {
            vec![]
        };

        let (published_bytecodes, mut tx_result) =
            vm.inspect_transaction_with_bytecode_compression(tracer.into(), tx.clone(), true);
        if published_bytecodes.is_ok() {
            let compressed_bytecodes = vm.get_last_tx_compressed_bytecodes();
            let calls = Arc::try_unwrap(call_tracer_result)
                .map_err(|_| anyhow::anyhow!("failed extracting call traces"))?
                .take()
                .unwrap_or_default();
            Ok(TransactionOutput {
                tx_result,
                compressed_bytecodes,
                calls,
            })
        } else {
            // Transaction failed to publish bytecodes, we reject it so initiator doesn't pay fee.
            tx_result.result = ExecutionResult::Halt {
                reason: Halt::FailedToPublishCompressedBytecodes,
            };
            Ok(TransactionOutput {
                tx_result,
                compressed_bytecodes: vec![],
                calls: vec![],
            })
        }
    }
}
