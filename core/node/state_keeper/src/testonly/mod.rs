//! Test utilities that can be used for testing sequencer that may
//! be useful outside of this crate.

use async_trait::async_trait;
use multivm::{
    interface::{
        CurrentExecutionState, ExecutionResult, FinishedL1Batch, L1BatchEnv, Refunds, SystemEnv,
        VmExecutionResultAndLogs, VmExecutionStatistics,
    },
    vm_latest::VmExecutionLogs,
};
use once_cell::sync::Lazy;
use tokio::sync::{mpsc, watch};
use zksync_contracts::BaseSystemContracts;

use crate::{
    batch_executor::{BatchExecutor, BatchExecutorHandle, Command, TxExecutionResult},
    types::ExecutionMetricsForCriteria,
};

pub mod test_batch_executor;

pub(super) static BASE_SYSTEM_CONTRACTS: Lazy<BaseSystemContracts> =
    Lazy::new(BaseSystemContracts::load_from_disk);

pub(super) fn default_vm_batch_result() -> FinishedL1Batch {
    FinishedL1Batch {
        block_tip_execution_result: VmExecutionResultAndLogs {
            result: ExecutionResult::Success { output: vec![] },
            logs: VmExecutionLogs::default(),
            statistics: VmExecutionStatistics::default(),
            refunds: Refunds::default(),
        },
        final_execution_state: CurrentExecutionState {
            events: vec![],
            deduplicated_storage_log_queries: vec![],
            used_contract_hashes: vec![],
            user_l2_to_l1_logs: vec![],
            system_logs: vec![],
            total_log_queries: 0,
            cycles_used: 0,
            deduplicated_events_logs: vec![],
            storage_refunds: Vec::new(),
            pubdata_costs: Vec::new(),
        },
        final_bootloader_memory: Some(vec![]),
        pubdata_input: Some(vec![]),
        initially_written_slots: Some(vec![]),
    }
}

/// Creates a `TxExecutionResult` object denoting a successful tx execution.
pub(crate) fn successful_exec() -> TxExecutionResult {
    TxExecutionResult::Success {
        tx_result: Box::new(VmExecutionResultAndLogs {
            result: ExecutionResult::Success { output: vec![] },
            logs: Default::default(),
            statistics: Default::default(),
            refunds: Default::default(),
        }),
        tx_metrics: Box::new(ExecutionMetricsForCriteria {
            l1_gas: Default::default(),
            execution_metrics: Default::default(),
        }),
        compressed_bytecodes: vec![],
        call_tracer_result: vec![],
        gas_remaining: Default::default(),
    }
}

/// `BatchExecutor` which doesn't check anything at all. Accepts all transactions.
#[derive(Debug)]
pub struct MockBatchExecutor;

#[async_trait]
impl BatchExecutor for MockBatchExecutor {
    async fn init_batch(
        &mut self,
        _l1batch_params: L1BatchEnv,
        _system_env: SystemEnv,
        _stop_receiver: &watch::Receiver<bool>,
    ) -> Option<BatchExecutorHandle> {
        let (send, recv) = mpsc::channel(1);
        let handle = tokio::task::spawn(async {
            let mut recv = recv;
            while let Some(cmd) = recv.recv().await {
                match cmd {
                    Command::ExecuteTx(_, resp) => resp.send(successful_exec()).unwrap(),
                    Command::StartNextL2Block(_, resp) => resp.send(()).unwrap(),
                    Command::RollbackLastTx(_) => panic!("unexpected rollback"),
                    Command::FinishBatch(resp) => {
                        // Blanket result, it doesn't really matter.
                        resp.send(default_vm_batch_result()).unwrap();
                        return;
                    }
                }
            }
        });
        Some(BatchExecutorHandle::from_raw(handle, send))
    }
}
