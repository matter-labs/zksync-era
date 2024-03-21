use std::sync::Arc;

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
use tokio::sync::{mpsc, watch};
use zksync_dal::{ConnectionPool, Core};
use zksync_state::{RocksdbStorage, StorageView, WriteStorage};
use zksync_types::{vm_trace::Call, Transaction, U256};
use zksync_utils::bytecode::CompressedBytecodeInfo;

use super::{BatchExecutor, BatchExecutorHandle, Command, TxExecutionResult};
use crate::{
    metrics::{InteractionType, TxStage, APP_METRICS},
    state_keeper::{
        metrics::{TxExecutionStage, BATCH_TIP_METRICS, EXECUTOR_METRICS, KEEPER_METRICS},
        types::ExecutionMetricsForCriteria,
    },
};

/// The default implementation of [`BatchExecutor`].
/// Creates a "real" batch executor which maintains the VM (as opposed to the test builder which doesn't use the VM).
#[derive(Debug, Clone)]
pub struct MainBatchExecutor {
    state_keeper_db_path: String,
    pool: ConnectionPool<Core>,
    save_call_traces: bool,
    max_allowed_tx_gas_limit: U256,
    upload_witness_inputs_to_gcs: bool,
    enum_index_migration_chunk_size: usize,
    optional_bytecode_compression: bool,
}

impl MainBatchExecutor {
    pub fn new(
        state_keeper_db_path: String,
        pool: ConnectionPool<Core>,
        max_allowed_tx_gas_limit: U256,
        save_call_traces: bool,
        upload_witness_inputs_to_gcs: bool,
        enum_index_migration_chunk_size: usize,
        optional_bytecode_compression: bool,
    ) -> Self {
        Self {
            state_keeper_db_path,
            pool,
            save_call_traces,
            max_allowed_tx_gas_limit,
            upload_witness_inputs_to_gcs,
            enum_index_migration_chunk_size,
            optional_bytecode_compression,
        }
    }
}

#[async_trait]
impl BatchExecutor for MainBatchExecutor {
    async fn init_batch(
        &mut self,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Option<BatchExecutorHandle> {
        let mut secondary_storage = RocksdbStorage::builder(self.state_keeper_db_path.as_ref())
            .await
            .expect("Failed initializing state keeper storage");
        secondary_storage.enable_enum_index_migration(self.enum_index_migration_chunk_size);
        let mut conn = self.pool.connection_tagged("state_keeper").await.unwrap();
        let secondary_storage = secondary_storage
            .synchronize(&mut conn, stop_receiver)
            .await
            .expect("Failed synchronizing secondary state keeper storage")?;

        // Since we process `BatchExecutor` commands one-by-one (the next command is never enqueued
        // until a previous command is processed), capacity 1 is enough for the commands channel.
        let (commands_sender, commands_receiver) = mpsc::channel(1);
        let executor = CommandReceiver {
            save_call_traces: self.save_call_traces,
            max_allowed_tx_gas_limit: self.max_allowed_tx_gas_limit,
            optional_bytecode_compression: self.optional_bytecode_compression,
            commands: commands_receiver,
        };
        let upload_witness_inputs_to_gcs = self.upload_witness_inputs_to_gcs;

        let handle = tokio::task::spawn_blocking(move || {
            executor.run(
                secondary_storage,
                l1_batch_params,
                system_env,
                upload_witness_inputs_to_gcs,
            )
        });
        Some(BatchExecutorHandle {
            handle,
            commands: commands_sender,
        })
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
    max_allowed_tx_gas_limit: U256,
    optional_bytecode_compression: bool,
    commands: mpsc::Receiver<Command>,
}

impl CommandReceiver {
    pub(super) fn run(
        mut self,
        secondary_storage: RocksdbStorage,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
        upload_witness_inputs_to_gcs: bool,
    ) {
        tracing::info!("Starting executing batch #{:?}", &l1_batch_params.number);

        let storage_view = StorageView::new(secondary_storage).to_rc_ptr();

        let mut vm = VmInstance::new(l1_batch_params, system_env, storage_view.clone());

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
                Command::StartNextMiniblock(l2_block_env, resp) => {
                    self.start_next_miniblock(l2_block_env, &mut vm);
                    resp.send(()).unwrap();
                }
                Command::FinishBatch(resp) => {
                    let vm_block_result = self.finish_batch(&mut vm);
                    let witness_block_state = if upload_witness_inputs_to_gcs {
                        Some(storage_view.borrow_mut().witness_block_state())
                    } else {
                        None
                    };
                    resp.send((vm_block_result, witness_block_state)).unwrap();

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
        tracing::info!("State keeper exited with an unfinished batch");
    }

    fn execute_tx<S: WriteStorage>(
        &self,
        tx: &Transaction,
        vm: &mut VmInstance<S, HistoryEnabled>,
    ) -> TxExecutionResult {
        // Save pre-`execute_next_tx` VM snapshot.
        vm.make_snapshot();

        // Reject transactions with too big gas limit.
        // They are also rejected on the API level, but
        // we need to secure ourselves in case some tx will somehow get into mempool.
        if tx.gas_limit() > self.max_allowed_tx_gas_limit {
            tracing::warn!(
                "Found tx with too big gas limit in state keeper, hash: {:?}, gas_limit: {}",
                tx.hash(),
                tx.gas_limit()
            );
            return TxExecutionResult::RejectedByVm {
                reason: Halt::TooBigGasLimit,
            };
        }

        // Execute the transaction.
        let latency = KEEPER_METRICS.tx_execution_time[&TxExecutionStage::Execution].start();
        let (tx_result, compressed_bytecodes, call_tracer_result) =
            if self.optional_bytecode_compression {
                self.execute_tx_in_vm_with_optional_compression(tx, vm)
            } else {
                self.execute_tx_in_vm(tx, vm)
            };
        latency.observe();
        APP_METRICS.processed_txs[&TxStage::StateKeeper].inc();
        APP_METRICS.processed_l1_txs[&TxStage::StateKeeper].inc_by(tx.is_l1().into());

        if let ExecutionResult::Halt { reason } = tx_result.result {
            return match reason {
                Halt::BootloaderOutOfGas => TxExecutionResult::BootloaderOutOfGasForTx,
                _ => TxExecutionResult::RejectedByVm { reason },
            };
        }

        let tx_metrics = ExecutionMetricsForCriteria::new(Some(tx), &tx_result);
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

    fn start_next_miniblock<S: WriteStorage>(
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

    // Err when transaction is rejected.
    // `Ok(TxExecutionStatus::Success)` when the transaction succeeded
    // `Ok(TxExecutionStatus::Failure)` when the transaction failed.
    // Note that failed transactions are considered properly processed and are included in blocks
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
