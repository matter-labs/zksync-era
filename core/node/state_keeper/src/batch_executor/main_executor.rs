use std::sync::Arc;

use anyhow::Context as _;
use tokio::{
    runtime::Handle,
    sync::{mpsc, watch},
};
use zksync_multivm::interface::{FinishedL1Batch, L1BatchEnv, SystemEnv};
use zksync_shared_metrics::{InteractionType, TxStage, APP_METRICS};
use zksync_state::{PgOrRocksdbStorage, ReadStorage, ReadStorageFactory, StoragePtr, StorageView};
use zksync_types::{L1BatchNumber, Transaction};

use super::{
    traits::{BatchVm, TraceCalls},
    BatchExecutorHandle, Command, TxExecutionResult,
};
use crate::{
    batch_executor::traits::{BatchVmFactory, DefaultBatchVmFactory},
    metrics::{TxExecutionStage, BATCH_TIP_METRICS, EXECUTOR_METRICS, KEEPER_METRICS},
};

/// Concrete trait object for the [`BatchVmFactory`] used in [`BatchExecutor`].
pub type DynVmFactory = dyn for<'a> BatchVmFactory<PgOrRocksdbStorage<'a>>;

/// Batch executor which maintains the VM for the duration of a single L1 batch.
#[derive(Debug)]
pub struct BatchExecutor {
    vm_factory: Arc<DynVmFactory>,
    trace_calls: TraceCalls,
    /// Whether batch executor would allow transactions with bytecode that cannot be compressed.
    /// For new blocks, bytecode compression is mandatory -- if bytecode compression is not supported,
    /// the transaction will be rejected.
    /// Note that this flag, if set to `true`, is strictly more permissive than if set to `false`. It means
    /// that in cases where the node is expected to process any transactions processed by the sequencer
    /// regardless of its configuration, this flag should be set to `true`.
    optional_bytecode_compression: bool,
}

impl BatchExecutor {
    /// Creates an executor with the specified parameters.
    pub fn new(save_call_traces: bool, optional_bytecode_compression: bool) -> Self {
        Self {
            vm_factory: Arc::new(DefaultBatchVmFactory),
            trace_calls: if save_call_traces {
                TraceCalls::Trace
            } else {
                TraceCalls::Skip
            },
            optional_bytecode_compression,
        }
    }

    /// Sets the VM factory used by this executor.
    pub fn with_vm_factory(mut self, vm_factory: Arc<DynVmFactory>) -> Self {
        self.vm_factory = vm_factory;
        self
    }

    pub async fn init_batch(
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
            trace_calls: self.trace_calls,
            optional_bytecode_compression: self.optional_bytecode_compression,
            commands: commands_receiver,
        };
        let vm_factory = self.vm_factory.clone();

        let stop_receiver = stop_receiver.clone();
        let handle = tokio::task::spawn_blocking(move || {
            let l1_batch_number = l1_batch_params.number;
            if let Some(storage) = Handle::current()
                .block_on(storage_factory.access_storage(&stop_receiver, l1_batch_number - 1))
                .context("failed accessing state keeper storage")?
            {
                let storage_view = StorageView::new(storage).to_rc_ptr();
                let vm = vm_factory.create_vm(l1_batch_params, system_env, storage_view.clone());
                executor.run(storage_view, l1_batch_number, vm);
            } else {
                tracing::info!("Interrupted while trying to access state keeper storage");
            }
            anyhow::Ok(())
        });
        Some(BatchExecutorHandle::from_raw(handle, commands_sender))
    }
}

/// Command processor for `BatchExecutor`.
/// Upon launch, it initializes the VM object with provided L1 batch context and properties, and keeps invoking the commands
/// sent to it one by one until the batch is finished.
///
/// One `CommandReceiver` can execute exactly one batch, so once the batch is sealed, a new `CommandReceiver` object must
/// be constructed.
#[derive(Debug)]
struct CommandReceiver {
    trace_calls: TraceCalls,
    optional_bytecode_compression: bool,
    commands: mpsc::Receiver<Command>,
}

impl CommandReceiver {
    fn run<S: ReadStorage>(
        mut self,
        storage_view: StoragePtr<StorageView<S>>,
        l1_batch_number: L1BatchNumber,
        mut vm: Box<dyn BatchVm + '_>,
    ) {
        tracing::info!("Starting executing L1 batch #{l1_batch_number}");

        while let Some(cmd) = self.commands.blocking_recv() {
            match cmd {
                Command::ExecuteTx(tx, resp) => {
                    let result = self.execute_tx(&tx, &mut *vm);
                    if resp.send(result).is_err() {
                        break;
                    }
                }
                Command::RollbackLastTx(resp) => {
                    self.rollback_last_tx(&mut *vm);
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
                    let vm_block_result = self.finish_batch(&mut *vm);
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
                Command::FinishBatchWithCache(resp) => {
                    let vm_block_result = self.finish_batch(&mut *vm);
                    let cache = (*storage_view).borrow().cache();
                    if resp.send((vm_block_result, cache)).is_err() {
                        break;
                    }

                    return;
                }
            }
        }
        // State keeper can exit because of stop signal, so it's OK to exit mid-batch.
        tracing::info!("State keeper exited with an unfinished L1 batch");
    }

    fn execute_tx(&self, tx: &Transaction, vm: &mut dyn BatchVm) -> TxExecutionResult {
        // Execute the transaction.
        let latency = KEEPER_METRICS.tx_execution_time[&TxExecutionStage::Execution].start();
        let vm_output = if self.optional_bytecode_compression {
            vm.execute_transaction_with_optional_compression(tx.clone(), self.trace_calls)
        } else {
            vm.execute_transaction(tx.clone(), self.trace_calls)
        };
        latency.observe();
        APP_METRICS.processed_txs[&TxStage::StateKeeper].inc();
        APP_METRICS.processed_l1_txs[&TxStage::StateKeeper].inc_by(tx.is_l1().into());

        vm_output
    }

    fn rollback_last_tx(&self, vm: &mut dyn BatchVm) {
        let latency = KEEPER_METRICS.tx_execution_time[&TxExecutionStage::TxRollback].start();
        vm.rollback_last_transaction();
        latency.observe();
    }

    fn finish_batch(&self, vm: &mut dyn BatchVm) -> FinishedL1Batch {
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
}
