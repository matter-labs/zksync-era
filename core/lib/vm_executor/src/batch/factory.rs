use std::{marker::PhantomData, rc::Rc, sync::Arc};

use anyhow::Context as _;
use once_cell::sync::{Lazy, OnceCell};
use tokio::sync::mpsc;
use zksync_multivm::{
    interface::{
        executor::{BatchExecutor, BatchExecutorFactory},
        storage::{ReadStorage, StorageView},
        BatchTransactionExecutionResult, ExecutionResult, FinishedL1Batch, Halt, L1BatchEnv,
        L2BlockEnv, SystemEnv, VmEvent, VmInterface, VmInterfaceHistoryEnabled,
    },
    tracers::CallTracer,
    vm_latest::HistoryEnabled,
    MultiVMTracer, VmInstance,
};
use zksync_types::{ethabi, vm::FastVmMode, Transaction, H256, KNOWN_CODES_STORAGE_ADDRESS};

use super::{
    executor::{Command, MainBatchExecutor},
    metrics::{TxExecutionStage, BATCH_TIP_METRICS, KEEPER_METRICS},
};
use crate::batch::metrics::{InteractionType, EXECUTOR_METRICS};

/// Extracts all bytecodes marked as known on the system contracts.
pub fn extract_bytecodes_marked_as_known(all_generated_events: &[VmEvent]) -> Vec<H256> {
    static PUBLISHED_BYTECODE_SIGNATURE: Lazy<H256> = Lazy::new(|| {
        ethabi::long_signature(
            "MarkedAsKnown",
            &[ethabi::ParamType::FixedBytes(32), ethabi::ParamType::Bool],
        )
    });

    all_generated_events
        .iter()
        .filter(|event| {
            // Filter events from the deployer contract that match the expected signature.
            event.address == KNOWN_CODES_STORAGE_ADDRESS
                && event.indexed_topics.len() == 3
                && event.indexed_topics[0] == *PUBLISHED_BYTECODE_SIGNATURE
        })
        .map(|event| event.indexed_topics[1])
        .collect()
}

/// The default implementation of [`BatchExecutorFactory`].
/// Creates real batch executors which maintain the VM (as opposed to the test factories which don't use the VM).
#[derive(Debug, Clone)]
pub struct MainBatchExecutorFactory {
    save_call_traces: bool,
    /// Whether batch executor would allow transactions with bytecode that cannot be compressed.
    /// For new blocks, bytecode compression is mandatory -- if bytecode compression is not supported,
    /// the transaction will be rejected.
    /// Note that this flag, if set to `true`, is strictly more permissive than if set to `false`. It means
    /// that in cases where the node is expected to process any transactions processed by the sequencer
    /// regardless of its configuration, this flag should be set to `true`.
    optional_bytecode_compression: bool,
    fast_vm_mode: FastVmMode,
}

impl MainBatchExecutorFactory {
    pub fn new(save_call_traces: bool, optional_bytecode_compression: bool) -> Self {
        Self {
            save_call_traces,
            optional_bytecode_compression,
            fast_vm_mode: FastVmMode::Old,
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
}

impl<S: ReadStorage + Send + 'static> BatchExecutorFactory<S> for MainBatchExecutorFactory {
    fn init_batch(
        &mut self,
        storage: S,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
    ) -> Box<dyn BatchExecutor<S>> {
        // Since we process `BatchExecutor` commands one-by-one (the next command is never enqueued
        // until a previous command is processed), capacity 1 is enough for the commands channel.
        let (commands_sender, commands_receiver) = mpsc::channel(1);
        let executor = CommandReceiver {
            save_call_traces: self.save_call_traces,
            optional_bytecode_compression: self.optional_bytecode_compression,
            fast_vm_mode: self.fast_vm_mode,
            commands: commands_receiver,
            _storage: PhantomData,
        };

        let handle =
            tokio::task::spawn_blocking(move || executor.run(storage, l1_batch_params, system_env));
        Box::new(MainBatchExecutor::new(handle, commands_sender))
    }
}

/// Implementation of the "primary" (non-test) batch executor.
/// Upon launch, it initializes the VM object with provided block context and properties, and keeps invoking the commands
/// sent to it one by one until the batch is finished.
///
/// One `CommandReceiver` can execute exactly one batch, so once the batch is sealed, a new `CommandReceiver` object must
/// be constructed.
#[derive(Debug)]
struct CommandReceiver<S> {
    save_call_traces: bool,
    optional_bytecode_compression: bool,
    fast_vm_mode: FastVmMode,
    commands: mpsc::Receiver<Command>,
    _storage: PhantomData<S>,
}

impl<S: ReadStorage + 'static> CommandReceiver<S> {
    pub(super) fn run(
        mut self,
        storage: S,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
    ) -> anyhow::Result<StorageView<S>> {
        tracing::info!("Starting executing L1 batch #{}", &l1_batch_params.number);

        let storage_view = StorageView::new(storage).to_rc_ptr();
        let mut vm = VmInstance::maybe_fast(
            l1_batch_params,
            system_env,
            storage_view.clone(),
            self.fast_vm_mode,
        );
        let mut batch_finished = false;

        while let Some(cmd) = self.commands.blocking_recv() {
            match cmd {
                Command::ExecuteTx(tx, resp) => {
                    let tx_hash = tx.hash();
                    let result = self.execute_tx(*tx, &mut vm).with_context(|| {
                        format!("fatal error executing transaction {tx_hash:?}")
                    })?;
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
                    batch_finished = true;
                    break;
                }
            }
        }

        drop(vm);
        let storage_view = Rc::into_inner(storage_view)
            .context("storage view leaked")?
            .into_inner();
        if batch_finished {
            let metrics = storage_view.metrics();
            EXECUTOR_METRICS.batch_storage_interaction_duration[&InteractionType::GetValue]
                .observe(metrics.time_spent_on_get_value);
            EXECUTOR_METRICS.batch_storage_interaction_duration[&InteractionType::SetValue]
                .observe(metrics.time_spent_on_set_value);
        } else {
            // State keeper can exit because of stop signal, so it's OK to exit mid-batch.
            tracing::info!("State keeper exited with an unfinished L1 batch");
        }
        Ok(storage_view)
    }

    fn execute_tx(
        &self,
        transaction: Transaction,
        vm: &mut VmInstance<S, HistoryEnabled>,
    ) -> anyhow::Result<BatchTransactionExecutionResult> {
        // Executing a next transaction means that a previous transaction was either rolled back (in which case its snapshot
        // was already removed), or that we build on top of it (in which case, it can be removed now).
        vm.pop_snapshot_no_rollback();
        // Save pre-execution VM snapshot.
        vm.make_snapshot();

        // Execute the transaction.
        let latency = KEEPER_METRICS.tx_execution_time[&TxExecutionStage::Execution].start();
        let result = if self.optional_bytecode_compression {
            self.execute_tx_in_vm_with_optional_compression(&transaction, vm)?
        } else {
            self.execute_tx_in_vm(&transaction, vm)?
        };

        latency.observe();

        Ok(result)
    }

    fn rollback_last_tx(&self, vm: &mut VmInstance<S, HistoryEnabled>) {
        let latency = KEEPER_METRICS.tx_execution_time[&TxExecutionStage::TxRollback].start();
        vm.rollback_to_the_latest_snapshot();
        latency.observe();
    }

    fn start_next_l2_block(
        &self,
        l2_block_env: L2BlockEnv,
        vm: &mut VmInstance<S, HistoryEnabled>,
    ) {
        vm.start_new_l2_block(l2_block_env);
    }

    fn finish_batch(
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
    fn execute_tx_in_vm_with_optional_compression(
        &self,
        tx: &Transaction,
        vm: &mut VmInstance<S, HistoryEnabled>,
    ) -> anyhow::Result<BatchTransactionExecutionResult> {
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

        if let (Ok(compressed_bytecodes), tx_result) =
            vm.inspect_transaction_with_bytecode_compression(tracer.into(), tx.clone(), true)
        {
            let factory_deps_marked_as_known =
                extract_bytecodes_marked_as_known(&tx_result.logs.events);
            let preimages = vm.ask_decommitter(factory_deps_marked_as_known.clone());
            let new_known_factory_deps = factory_deps_marked_as_known
                .into_iter()
                .zip(preimages)
                .collect();
            let call_traces = Arc::try_unwrap(call_tracer_result)
                .map_err(|_| anyhow::anyhow!("failed extracting call traces"))?
                .take()
                .unwrap_or_default();
            return Ok(BatchTransactionExecutionResult {
                tx_result: Box::new(tx_result),
                compressed_bytecodes,
                call_traces,
                new_known_factory_deps,
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

        let (compression_result, mut tx_result) =
            vm.inspect_transaction_with_bytecode_compression(tracer.into(), tx.clone(), false);
        let compressed_bytecodes =
            compression_result.context("compression failed when it wasn't applied")?;

        let factory_deps_marked_as_known =
            extract_bytecodes_marked_as_known(&tx_result.logs.events);
        let preimages = vm.ask_decommitter(factory_deps_marked_as_known.clone());
        let new_known_factory_deps = factory_deps_marked_as_known
            .into_iter()
            .zip(preimages)
            .collect();

        // TODO implement tracer manager which will be responsible
        //   for collecting result from all tracers and save it to the database
        let call_traces = Arc::try_unwrap(call_tracer_result)
            .map_err(|_| anyhow::anyhow!("failed extracting call traces"))?
            .take()
            .unwrap_or_default();
        Ok(BatchTransactionExecutionResult {
            tx_result: Box::new(tx_result),
            compressed_bytecodes,
            call_traces,
            new_known_factory_deps,
        })
    }

    /// Attempts to execute transaction with mandatory bytecode compression.
    /// If bytecode compression fails, the transaction will be rejected.
    fn execute_tx_in_vm(
        &self,
        tx: &Transaction,
        vm: &mut VmInstance<S, HistoryEnabled>,
    ) -> anyhow::Result<BatchTransactionExecutionResult> {
        let call_tracer_result = Arc::new(OnceCell::default());
        let tracer = if self.save_call_traces {
            vec![CallTracer::new(call_tracer_result.clone()).into_tracer_pointer()]
        } else {
            vec![]
        };

        let (bytecodes_result, mut tx_result) =
            vm.inspect_transaction_with_bytecode_compression(tracer.into(), tx.clone(), true);
        if let Ok(compressed_bytecodes) = bytecodes_result {
            let factory_deps_marked_as_known =
                extract_bytecodes_marked_as_known(&tx_result.logs.events);
            let preimages = vm.ask_decommitter(factory_deps_marked_as_known.clone());
            let new_known_factory_deps = factory_deps_marked_as_known
                .into_iter()
                .zip(preimages)
                .collect();
            let call_traces = Arc::try_unwrap(call_tracer_result)
                .map_err(|_| anyhow::anyhow!("failed extracting call traces"))?
                .take()
                .unwrap_or_default();
            Ok(BatchTransactionExecutionResult {
                tx_result: Box::new(tx_result),
                compressed_bytecodes,
                call_traces,
                new_known_factory_deps,
            })
        } else {
            // Transaction failed to publish bytecodes, we reject it so initiator doesn't pay fee.
            tx_result.result = ExecutionResult::Halt {
                reason: Halt::FailedToPublishCompressedBytecodes,
            };
            Ok(BatchTransactionExecutionResult {
                tx_result: Box::new(tx_result),
                compressed_bytecodes: vec![],
                call_traces: vec![],
                new_known_factory_deps: vec![],
            })
        }
    }
}
