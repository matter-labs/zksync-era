//! Helper traits for the main executor.

use std::{fmt, sync::Arc};

use once_cell::sync::OnceCell;
use zksync_multivm::{
    interface::{
        ExecutionResult, FinishedL1Batch, Halt, L1BatchEnv, L2BlockEnv, SystemEnv,
        VmExecutionResultAndLogs, VmFactory, VmInterface, VmInterfaceHistoryEnabled,
    },
    tracers::CallTracer,
    vm_latest::HistoryEnabled,
    MultiVMTracer, VmInstance,
};
use zksync_state::{ReadStorage, StoragePtr, StorageView};
use zksync_types::{vm_trace::Call, Transaction};
use zksync_utils::bytecode::CompressedBytecodeInfo;

use crate::{ExecutionMetricsForCriteria, TxExecutionResult};

#[derive(Debug, Clone, Copy)]
pub(super) enum TraceCalls {
    Trace,
    Skip,
}

#[derive(Debug)]
pub(super) struct VmTransactionOutput {
    tx_result: VmExecutionResultAndLogs,
    compressed_bytecodes: Vec<CompressedBytecodeInfo>,
    /// Empty if call tracing was not requested.
    call_tracer_result: Vec<Call>,
    gas_remaining: u32,
}

impl VmTransactionOutput {
    fn from_err(err: VmTransactionError) -> Self {
        let mut tx_result = *err.tx_result;
        tx_result.result = ExecutionResult::Halt {
            reason: Halt::FailedToPublishCompressedBytecodes,
        };
        Self {
            tx_result,
            compressed_bytecodes: vec![],
            call_tracer_result: vec![],
            gas_remaining: 0, // is not used; see below
        }
    }

    pub(super) fn into_tx_result(self, tx: &Transaction) -> TxExecutionResult {
        if let ExecutionResult::Halt { reason } = self.tx_result.result {
            return match reason {
                Halt::BootloaderOutOfGas => TxExecutionResult::BootloaderOutOfGasForTx,
                _ => TxExecutionResult::RejectedByVm { reason },
            };
        }

        let tx_metrics = ExecutionMetricsForCriteria::new(Some(tx), &self.tx_result);

        TxExecutionResult::Success {
            tx_result: Box::new(self.tx_result),
            tx_metrics: Box::new(tx_metrics),
            compressed_bytecodes: self.compressed_bytecodes,
            call_tracer_result: self.call_tracer_result,
            gas_remaining: self.gas_remaining,
        }
    }
}

#[derive(Debug)]
pub(super) struct VmTransactionError {
    tx_result: Box<VmExecutionResultAndLogs>,
}

/// Object-safe version of `VmInterface`.
///
/// # Invariants
///
/// - Transaction inspection methods must create a VM snapshot before the execution, and must not roll back these snapshots.
///
pub(super) trait VmWithCallTracing {
    /// Attempts to execute transaction with mandatory bytecode compression.
    /// If bytecode compression fails, the transaction will be rejected.
    fn inspect_transaction_without_compression(
        &mut self,
        tx: Transaction,
        trace_calls: TraceCalls,
    ) -> VmTransactionOutput;

    /// Attempts to execute transaction with or without bytecode compression.
    /// If compression fails, the transaction will be re-executed without compression.
    fn inspect_transaction_with_optional_compression(
        &mut self,
        tx: Transaction,
        trace_calls: TraceCalls,
    ) -> VmTransactionOutput;

    fn rollback_last_transaction(&mut self);

    fn start_new_l2_block(&mut self, l2_block: L2BlockEnv);

    fn finish_batch(&mut self) -> FinishedL1Batch;
}

fn inspect_transaction<S: ReadStorage>(
    vm: &mut VmInstance<S, HistoryEnabled>,
    tx: Transaction,
    trace_calls: TraceCalls,
    compress_bytecodes: bool,
) -> Result<VmTransactionOutput, VmTransactionError> {
    let call_tracer_result = Arc::new(OnceCell::default());
    let tracer = match trace_calls {
        TraceCalls::Trace => {
            vec![CallTracer::new(call_tracer_result.clone()).into_tracer_pointer()]
        }
        TraceCalls::Skip => vec![],
    };

    let (compression_result, tx_result) = vm.inspect_transaction_with_bytecode_compression(
        tracer.into(),
        tx.clone(),
        compress_bytecodes,
    );
    if compression_result.is_err() {
        return Err(VmTransactionError {
            tx_result: Box::new(tx_result),
        });
    }

    let call_tracer_result = Arc::try_unwrap(call_tracer_result)
        .unwrap()
        .take()
        .unwrap_or_default();
    Ok(VmTransactionOutput {
        gas_remaining: if matches!(tx_result.result, ExecutionResult::Halt { .. }) {
            0 // FIXME: will not be used; currently, the new VM diverges in this case
        } else {
            vm.gas_remaining()
        },
        tx_result,
        compressed_bytecodes: vm.get_last_tx_compressed_bytecodes(),
        call_tracer_result,
    })
}

impl<S: ReadStorage> VmWithCallTracing for VmInstance<S, HistoryEnabled> {
    fn inspect_transaction_without_compression(
        &mut self,
        tx: Transaction,
        trace_calls: TraceCalls,
    ) -> VmTransactionOutput {
        // Executing a next transaction means that a previous transaction was either rolled back (in which case its snapshot
        // was already removed), or that we build on top of it (in which case, it can be removed now).
        self.pop_snapshot_no_rollback();
        // Save pre-execution VM snapshot.
        self.make_snapshot();

        inspect_transaction(self, tx, trace_calls, false)
            .unwrap_or_else(VmTransactionOutput::from_err)
    }

    fn inspect_transaction_with_optional_compression(
        &mut self,
        tx: Transaction,
        trace_calls: TraceCalls,
    ) -> VmTransactionOutput {
        // Executing a next transaction means that a previous transaction was either rolled back (in which case its snapshot
        // was already removed), or that we build on top of it (in which case, it can be removed now).
        self.pop_snapshot_no_rollback();
        // Save pre-execution VM snapshot.
        self.make_snapshot();

        // Note, that the space where we can put the calldata for compressing transactions
        // is limited and the transactions do not pay for taking it.
        // In order to not let the accounts spam the space of compressed bytecodes with bytecodes
        // that will not be published (e.g. due to out of gas), we use the following scheme:
        // We try to execute the transaction with compressed bytecodes.
        // If it fails and the compressed bytecodes have not been published,
        // it means that there is no sense in polluting the space of compressed bytecodes,
        // and so we re-execute the transaction, but without compression.

        if let Ok(output) = inspect_transaction(self, tx.clone(), trace_calls, true) {
            return output;
        }

        // Roll back to the snapshot just before the transaction execution taken in `Self::execute_tx()`
        // and create a snapshot at the same VM state again.
        self.rollback_to_the_latest_snapshot();
        self.make_snapshot();

        inspect_transaction(self, tx.clone(), trace_calls, false)
            .expect("Compression can't fail if we don't apply it")
    }

    fn rollback_last_transaction(&mut self) {
        self.rollback_to_the_latest_snapshot();
    }

    fn start_new_l2_block(&mut self, l2_block: L2BlockEnv) {
        VmInterface::start_new_l2_block(self, l2_block);
    }

    fn finish_batch(&mut self) -> FinishedL1Batch {
        VmInterface::finish_batch(self)
    }
}

/// VM factory used by the main batch executor. Encapsulates the storage type used by the executor
/// so that it's opaque from the implementor's perspective.
pub(super) trait BatchVmFactory<S>: fmt::Debug + Send + Sync {
    fn create_vm<'a>(
        &self,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
        storage: StoragePtr<StorageView<S>>,
    ) -> Box<dyn VmWithCallTracing + 'a>
    where
        S: 'a;
}

/// Default VM factory.
impl<S: ReadStorage> BatchVmFactory<S> for () {
    fn create_vm<'a>(
        &self,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
        storage: StoragePtr<StorageView<S>>,
    ) -> Box<dyn VmWithCallTracing + 'a>
    where
        S: 'a,
    {
        Box::new(VmInstance::<_, HistoryEnabled>::new(
            l1_batch_params,
            system_env,
            storage,
        ))
    }
}
