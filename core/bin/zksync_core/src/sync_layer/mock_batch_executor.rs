//! This module provide a mock batch executor that in fact does not execute any transactions.
//! This is a stub that is helpful for the development of the External Node, as it allows to
//! not focus on the execution of the transactions, but rather only care about the data flow between
//! the fetcher and the state keeper.
//!
//! This is temporary module which will be removed once EN binary is more or less ready.
//! It also has a fair amount of copy-paste from the state keeper tests, which is OK, given that this module
//! is temporary and otherwise we would've had to make the state keeper tests public.

use std::sync::mpsc;

use vm::{
    vm::{VmPartialExecutionResult, VmTxExecutionResult},
    VmBlockResult, VmExecutionResult,
};
use zksync_types::tx::tx_execution_info::TxExecutionStatus;
use zksync_types::vm_trace::{VmExecutionTrace, VmTrace};

use crate::state_keeper::{
    batch_executor::{BatchExecutorHandle, Command, L1BatchExecutorBuilder, TxExecutionResult},
    io::L1BatchParams,
    types::ExecutionMetricsForCriteria,
};

#[derive(Debug)]
pub struct MockBatchExecutorBuilder;

impl L1BatchExecutorBuilder for MockBatchExecutorBuilder {
    fn init_batch(&self, _l1_batch_params: L1BatchParams) -> BatchExecutorHandle {
        let (tx, rx) = mpsc::channel::<Command>();
        let responder_thread_handle = std::thread::spawn(move || loop {
            let action = rx.recv().unwrap();
            match action {
                Command::ExecuteTx(_, resp) => {
                    resp.send(successful_exec()).unwrap();
                }
                Command::RollbackLastTx(_resp) => {
                    panic!("Rollback should never happen");
                }
                Command::FinishBatch(resp) => {
                    // Blanket result, it doesn't really matter.
                    let result = VmBlockResult {
                        full_result: VmExecutionResult {
                            events: Default::default(),
                            storage_log_queries: Default::default(),
                            used_contract_hashes: Default::default(),
                            l2_to_l1_logs: Default::default(),
                            return_data: Default::default(),
                            gas_used: Default::default(),
                            contracts_used: Default::default(),
                            revert_reason: Default::default(),
                            trace: VmTrace::ExecutionTrace(VmExecutionTrace::default()),
                            total_log_queries: Default::default(),
                            cycles_used: Default::default(),
                            computational_gas_used: Default::default(),
                        },
                        block_tip_result: VmPartialExecutionResult {
                            logs: Default::default(),
                            revert_reason: Default::default(),
                            contracts_used: Default::default(),
                            cycles_used: Default::default(),
                            computational_gas_used: Default::default(),
                        },
                    };

                    resp.send(result).unwrap();
                    break;
                }
            }
        });

        BatchExecutorHandle::from_raw(responder_thread_handle, tx)
    }
}

fn partial_execution_result() -> VmPartialExecutionResult {
    VmPartialExecutionResult {
        logs: Default::default(),
        revert_reason: Default::default(),
        contracts_used: Default::default(),
        cycles_used: Default::default(),
        computational_gas_used: Default::default(),
    }
}

/// Creates a `TxExecutionResult` object denoting a successful tx execution.
pub(crate) fn successful_exec() -> TxExecutionResult {
    TxExecutionResult::Success {
        tx_result: Box::new(VmTxExecutionResult {
            status: TxExecutionStatus::Success,
            result: partial_execution_result(),
            call_traces: vec![],
            gas_refunded: 0,
            operator_suggested_refund: 0,
        }),
        tx_metrics: ExecutionMetricsForCriteria {
            l1_gas: Default::default(),
            execution_metrics: Default::default(),
        },
        bootloader_dry_run_metrics: ExecutionMetricsForCriteria {
            l1_gas: Default::default(),
            execution_metrics: Default::default(),
        },
        bootloader_dry_run_result: Box::new(partial_execution_result()),
        compressed_bytecodes: vec![],
    }
}
