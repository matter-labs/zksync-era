use std::{borrow::Cow, fmt, marker::PhantomData, rc::Rc, sync::Arc, time::Duration};

use anyhow::Context as _;
use once_cell::sync::OnceCell;
use tokio::sync::mpsc;
use zksync_multivm::{
    interface::{
        executor::{BatchExecutor, BatchExecutorFactory},
        pubdata::PubdataBuilder,
        storage::{ReadStorage, StoragePtr, StorageView, StorageViewStats},
        utils::DivergenceHandler,
        BatchTransactionExecutionResult, BytecodeCompressionError, CompressedBytecodeInfo,
        ExecutionResult, FinishedL1Batch, Halt, L1BatchEnv, L2BlockEnv, SystemEnv, VmFactory,
        VmInterface, VmInterfaceHistoryEnabled,
    },
    is_supported_by_fast_vm,
    pubdata_builders::pubdata_params_to_builder,
    tracers::CallTracer,
    vm_fast,
    vm_latest::HistoryEnabled,
    FastVmInstance, LegacyVmInstance, MultiVMTracer,
};
use zksync_types::{commitment::PubdataParams, vm::FastVmMode, Transaction};

use super::{
    executor::{Command, MainBatchExecutor},
    metrics::{TxExecutionStage, BATCH_TIP_METRICS, EXECUTOR_METRICS, KEEPER_METRICS},
};
use crate::shared::{InteractionType, Sealed, STORAGE_METRICS};

/// Encapsulates a tracer used during batch processing. Currently supported tracers are `()` (no-op) and [`TraceCalls`].
///
/// All members of this trait are implementation details.
pub trait BatchTracer: fmt::Debug + 'static + Send + Sealed {
    /// True if call tracing is enabled. Used by legacy VMs which enable / disable call tracing dynamically.
    #[doc(hidden)]
    const TRACE_CALLS: bool;
    /// Tracer for the fast VM.
    #[doc(hidden)]
    type Fast: vm_fast::Tracer + Default + 'static;
}

impl Sealed for () {}

/// No-op implementation that doesn't trace anything.
impl BatchTracer for () {
    const TRACE_CALLS: bool = false;
    type Fast = ();
}

/// [`BatchTracer`] implementation tracing calls (returned in [`BatchTransactionExecutionResult`]s).
#[derive(Debug)]
pub struct TraceCalls(());

impl Sealed for TraceCalls {}

impl BatchTracer for TraceCalls {
    const TRACE_CALLS: bool = true;
    type Fast = (); // TODO: change once call tracing is implemented in fast VM
}

/// The default implementation of [`BatchExecutorFactory`].
/// Creates real batch executors which maintain the VM (as opposed to the test factories which don't use the VM).
#[derive(Debug, Clone)]
pub struct MainBatchExecutorFactory<Tr> {
    /// Whether batch executor would allow transactions with bytecode that cannot be compressed.
    /// For new blocks, bytecode compression is mandatory -- if bytecode compression is not supported,
    /// the transaction will be rejected.
    /// Note that this flag, if set to `true`, is strictly more permissive than if set to `false`. It means
    /// that in cases where the node is expected to process any transactions processed by the sequencer
    /// regardless of its configuration, this flag should be set to `true`.
    optional_bytecode_compression: bool,
    fast_vm_mode: FastVmMode,
    observe_storage_metrics: bool,
    divergence_handler: Option<DivergenceHandler>,
    _tracer: PhantomData<Tr>,
}

impl<Tr: BatchTracer> MainBatchExecutorFactory<Tr> {
    pub fn new(optional_bytecode_compression: bool) -> Self {
        Self {
            optional_bytecode_compression,
            fast_vm_mode: FastVmMode::Old,
            observe_storage_metrics: false,
            divergence_handler: None,
            _tracer: PhantomData,
        }
    }

    /// Sets the fast VM mode used by this executor.
    pub fn set_fast_vm_mode(&mut self, fast_vm_mode: FastVmMode) {
        if !matches!(fast_vm_mode, FastVmMode::Old) {
            tracing::warn!(
                "Running new VM with mode {fast_vm_mode:?}; this can lead to incorrect node behavior"
            );
        }
        self.fast_vm_mode = fast_vm_mode;
    }

    /// Enables storage metrics reporting for this executor. Storage metrics will be reported for each transaction.
    // The reason this isn't on by default is that storage metrics don't distinguish between "batch-executed" and "oneshot-executed" transactions;
    // this optimally needs some improvements in `vise` (ability to add labels for groups of metrics).
    pub fn observe_storage_metrics(&mut self) {
        self.observe_storage_metrics = true;
    }

    pub fn set_divergence_handler(&mut self, handler: DivergenceHandler) {
        tracing::info!("Set VM divergence handler");
        self.divergence_handler = Some(handler);
    }
}

impl<S: ReadStorage + Send + 'static, Tr: BatchTracer> BatchExecutorFactory<S>
    for MainBatchExecutorFactory<Tr>
{
    fn init_batch(
        &mut self,
        storage: S,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
        pubdata_params: PubdataParams,
    ) -> Box<dyn BatchExecutor<S>> {
        // Since we process `BatchExecutor` commands one-by-one (the next command is never enqueued
        // until a previous command is processed), capacity 1 is enough for the commands channel.
        let (commands_sender, commands_receiver) = mpsc::channel(1);
        let executor = CommandReceiver {
            optional_bytecode_compression: self.optional_bytecode_compression,
            fast_vm_mode: self.fast_vm_mode,
            observe_storage_metrics: self.observe_storage_metrics,
            divergence_handler: self.divergence_handler.clone(),
            commands: commands_receiver,
            _storage: PhantomData,
            _tracer: PhantomData::<Tr>,
        };

        let handle = tokio::task::spawn_blocking(move || {
            executor.run(
                storage,
                l1_batch_params,
                system_env,
                pubdata_params_to_builder(pubdata_params),
            )
        });
        Box::new(MainBatchExecutor::new(handle, commands_sender))
    }
}

type BytecodeResult = Result<Vec<CompressedBytecodeInfo>, BytecodeCompressionError>;

#[derive(Debug)]
enum BatchVm<S: ReadStorage, Tr: BatchTracer> {
    Legacy(LegacyVmInstance<S, HistoryEnabled>),
    Fast(FastVmInstance<S, Tr::Fast>),
}

macro_rules! dispatch_batch_vm {
    ($self:ident.$function:ident($($params:tt)*)) => {
        match $self {
            Self::Legacy(vm) => vm.$function($($params)*),
            Self::Fast(vm) => vm.$function($($params)*),
        }
    };
}

impl<S: ReadStorage, Tr: BatchTracer> BatchVm<S, Tr> {
    fn new(
        l1_batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage_ptr: StoragePtr<StorageView<S>>,
        mode: FastVmMode,
    ) -> Self {
        if !is_supported_by_fast_vm(system_env.version) {
            return Self::Legacy(LegacyVmInstance::new(l1_batch_env, system_env, storage_ptr));
        }

        match mode {
            FastVmMode::Old => {
                Self::Legacy(LegacyVmInstance::new(l1_batch_env, system_env, storage_ptr))
            }
            FastVmMode::New => {
                Self::Fast(FastVmInstance::fast(l1_batch_env, system_env, storage_ptr))
            }
            FastVmMode::Shadow => Self::Fast(FastVmInstance::shadowed(
                l1_batch_env,
                system_env,
                storage_ptr,
            )),
        }
    }

    fn start_new_l2_block(&mut self, l2_block: L2BlockEnv) {
        dispatch_batch_vm!(self.start_new_l2_block(l2_block));
    }

    fn finish_batch(&mut self, pubdata_builder: Rc<dyn PubdataBuilder>) -> FinishedL1Batch {
        dispatch_batch_vm!(self.finish_batch(pubdata_builder))
    }

    fn make_snapshot(&mut self) {
        dispatch_batch_vm!(self.make_snapshot());
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        dispatch_batch_vm!(self.rollback_to_the_latest_snapshot());
    }

    fn pop_snapshot_no_rollback(&mut self) {
        dispatch_batch_vm!(self.pop_snapshot_no_rollback());
    }

    fn inspect_transaction(
        &mut self,
        tx: Transaction,
        with_compression: bool,
    ) -> BatchTransactionExecutionResult<BytecodeResult> {
        let call_tracer_result = Arc::new(OnceCell::default());
        let legacy_tracer = if Tr::TRACE_CALLS {
            vec![CallTracer::new(call_tracer_result.clone()).into_tracer_pointer()]
        } else {
            vec![]
        };
        let mut legacy_tracer = legacy_tracer.into();

        let (compression_result, tx_result) = match self {
            Self::Legacy(vm) => vm.inspect_transaction_with_bytecode_compression(
                &mut legacy_tracer,
                tx,
                with_compression,
            ),
            Self::Fast(vm) => {
                let mut tracer = (legacy_tracer.into(), <Tr::Fast>::default());
                vm.inspect_transaction_with_bytecode_compression(&mut tracer, tx, with_compression)
            }
        };

        let compressed_bytecodes = compression_result.map(Cow::into_owned);
        let call_traces = Arc::try_unwrap(call_tracer_result)
            .expect("failed extracting call traces")
            .take()
            .unwrap_or_default();
        BatchTransactionExecutionResult {
            tx_result: Box::new(tx_result),
            compressed_bytecodes,
            call_traces,
        }
    }
}

/// Implementation of the "primary" (non-test) batch executor.
/// Upon launch, it initializes the VM object with provided block context and properties, and keeps invoking the commands
/// sent to it one by one until the batch is finished.
///
/// One `CommandReceiver` can execute exactly one batch, so once the batch is sealed, a new `CommandReceiver` object must
/// be constructed.
#[derive(Debug)]
struct CommandReceiver<S, Tr> {
    optional_bytecode_compression: bool,
    fast_vm_mode: FastVmMode,
    observe_storage_metrics: bool,
    divergence_handler: Option<DivergenceHandler>,
    commands: mpsc::Receiver<Command>,
    _storage: PhantomData<S>,
    _tracer: PhantomData<Tr>,
}

impl<S: ReadStorage + 'static, Tr: BatchTracer> CommandReceiver<S, Tr> {
    pub(super) fn run(
        mut self,
        storage: S,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
        pubdata_builder: Rc<dyn PubdataBuilder>,
    ) -> anyhow::Result<StorageView<S>> {
        tracing::info!("Starting executing L1 batch #{}", &l1_batch_params.number);

        let storage_view = StorageView::new(storage).to_rc_ptr();
        let mut vm = BatchVm::<S, Tr>::new(
            l1_batch_params,
            system_env,
            storage_view.clone(),
            self.fast_vm_mode,
        );
        let mut batch_finished = false;
        let mut prev_storage_stats = StorageViewStats::default();

        if let BatchVm::Fast(FastVmInstance::Shadowed(shadowed)) = &mut vm {
            if let Some(handler) = self.divergence_handler.take() {
                shadowed.set_divergence_handler(handler);
            }
        }

        while let Some(cmd) = self.commands.blocking_recv() {
            match cmd {
                Command::ExecuteTx(tx, resp) => {
                    let tx_hash = tx.hash();
                    let (result, latency) = self.execute_tx(*tx, &mut vm).with_context(|| {
                        format!("fatal error executing transaction {tx_hash:?}")
                    })?;

                    if self.observe_storage_metrics {
                        let storage_stats = storage_view.borrow().stats();
                        let stats_diff = storage_stats.saturating_sub(&prev_storage_stats);
                        STORAGE_METRICS.observe(&format!("Tx {tx_hash:?}"), latency, &stats_diff);
                        prev_storage_stats = storage_stats;
                    }
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
                    vm.start_new_l2_block(l2_block_env);
                    if resp.send(()).is_err() {
                        break;
                    }
                }
                Command::FinishBatch(resp) => {
                    let vm_block_result = self.finish_batch(&mut vm, pubdata_builder)?;
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
            let stats = storage_view.stats();
            EXECUTOR_METRICS.batch_storage_interaction_duration[&InteractionType::GetValue]
                .observe(stats.time_spent_on_get_value);
            EXECUTOR_METRICS.batch_storage_interaction_duration[&InteractionType::SetValue]
                .observe(stats.time_spent_on_set_value);
        } else {
            // State keeper can exit because of stop signal, so it's OK to exit mid-batch.
            tracing::info!("State keeper exited with an unfinished L1 batch");
        }
        Ok(storage_view)
    }

    fn execute_tx(
        &self,
        transaction: Transaction,
        vm: &mut BatchVm<S, Tr>,
    ) -> anyhow::Result<(BatchTransactionExecutionResult, Duration)> {
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

        Ok((result, latency.observe()))
    }

    fn rollback_last_tx(&self, vm: &mut BatchVm<S, Tr>) {
        let latency = KEEPER_METRICS.tx_execution_time[&TxExecutionStage::TxRollback].start();
        vm.rollback_to_the_latest_snapshot();
        latency.observe();
    }

    fn finish_batch(
        &self,
        vm: &mut BatchVm<S, Tr>,
        pubdata_builder: Rc<dyn PubdataBuilder>,
    ) -> anyhow::Result<FinishedL1Batch> {
        // The vm execution was paused right after the last transaction was executed.
        // There is some post-processing work that the VM needs to do before the block is fully processed.
        let result = vm.finish_batch(pubdata_builder);
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
        vm: &mut BatchVm<S, Tr>,
    ) -> anyhow::Result<BatchTransactionExecutionResult> {
        // Note, that the space where we can put the calldata for compressing transactions
        // is limited and the transactions do not pay for taking it.
        // In order to not let the accounts spam the space of compressed bytecodes with bytecodes
        // that will not be published (e.g. due to out of gas), we use the following scheme:
        // We try to execute the transaction with compressed bytecodes.
        // If it fails and the compressed bytecodes have not been published,
        // it means that there is no sense in polluting the space of compressed bytecodes,
        // and so we re-execute the transaction, but without compression.

        let res = vm.inspect_transaction(tx.clone(), true);
        if let Ok(compressed_bytecodes) = res.compressed_bytecodes {
            return Ok(BatchTransactionExecutionResult {
                tx_result: res.tx_result,
                compressed_bytecodes,
                call_traces: res.call_traces,
            });
        }

        // Roll back to the snapshot just before the transaction execution taken in `Self::execute_tx()`
        // and create a snapshot at the same VM state again.
        vm.rollback_to_the_latest_snapshot();
        vm.make_snapshot();

        let res = vm.inspect_transaction(tx.clone(), false);
        let compressed_bytecodes = res
            .compressed_bytecodes
            .context("compression failed when it wasn't applied")?;
        Ok(BatchTransactionExecutionResult {
            tx_result: res.tx_result,
            compressed_bytecodes,
            call_traces: res.call_traces,
        })
    }

    /// Attempts to execute transaction with mandatory bytecode compression.
    /// If bytecode compression fails, the transaction will be rejected.
    fn execute_tx_in_vm(
        &self,
        tx: &Transaction,
        vm: &mut BatchVm<S, Tr>,
    ) -> anyhow::Result<BatchTransactionExecutionResult> {
        let res = vm.inspect_transaction(tx.clone(), true);
        if let Ok(compressed_bytecodes) = res.compressed_bytecodes {
            Ok(BatchTransactionExecutionResult {
                tx_result: res.tx_result,
                compressed_bytecodes,
                call_traces: res.call_traces,
            })
        } else {
            // Transaction failed to publish bytecodes, we reject it so initiator doesn't pay fee.
            let mut tx_result = res.tx_result;
            tx_result.result = ExecutionResult::Halt {
                reason: Halt::FailedToPublishCompressedBytecodes,
            };
            Ok(BatchTransactionExecutionResult {
                tx_result,
                compressed_bytecodes: vec![],
                call_traces: vec![],
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use zksync_multivm::interface::{storage::InMemoryStorage, TxExecutionMode};
    use zksync_types::ProtocolVersionId;

    use super::*;
    use crate::testonly::{default_l1_batch_env, default_system_env, FAST_VM_MODES};

    #[test]
    fn selecting_vm_for_execution() {
        let l1_batch_env = default_l1_batch_env(1);
        let mut system_env = SystemEnv {
            version: ProtocolVersionId::Version22,
            ..default_system_env(TxExecutionMode::VerifyExecute)
        };
        let storage = StorageView::new(InMemoryStorage::default()).to_rc_ptr();
        for mode in FAST_VM_MODES {
            let vm = BatchVm::<_, ()>::new(
                l1_batch_env.clone(),
                system_env.clone(),
                storage.clone(),
                mode,
            );
            assert_matches!(vm, BatchVm::Legacy(_));
        }

        system_env.version = ProtocolVersionId::latest();
        let vm = BatchVm::<_, ()>::new(
            l1_batch_env.clone(),
            system_env.clone(),
            storage.clone(),
            FastVmMode::Old,
        );
        assert_matches!(vm, BatchVm::Legacy(_));
        let vm = BatchVm::<_, ()>::new(
            l1_batch_env.clone(),
            system_env.clone(),
            storage.clone(),
            FastVmMode::New,
        );
        assert_matches!(vm, BatchVm::Fast(FastVmInstance::Fast(_)));
        let vm = BatchVm::<_, ()>::new(l1_batch_env, system_env, storage, FastVmMode::Shadow);
        assert_matches!(vm, BatchVm::Fast(FastVmInstance::Shadowed(_)));
    }
}
