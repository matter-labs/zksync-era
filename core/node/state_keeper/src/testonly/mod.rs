//! Test utilities that can be used for testing sequencer that may
//! be useful outside of this crate.

use std::sync::Arc;

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
use zksync_dal::{ConnectionPool, Core, CoreDal as _};
use zksync_state::ReadStorageFactory;
use zksync_test_account::Account;
use zksync_types::{
    fee::Fee, utils::storage_key_for_standard_token_balance, AccountTreeId, Address, Execute,
    L1BatchNumber, L2BlockNumber, PriorityOpId, StorageLog, Transaction, H256,
    L2_BASE_TOKEN_ADDRESS, SYSTEM_CONTEXT_MINIMAL_BASE_FEE, U256,
};
use zksync_utils::u256_to_h256;

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
        state_diffs: Some(vec![]),
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
        _storage_factory: Arc<dyn ReadStorageFactory>,
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
                        break;
                    }
                }
            }
            anyhow::Ok(())
        });
        Some(BatchExecutorHandle::from_raw(handle, send))
    }
}

/// Adds funds for specified account list.
/// Expects genesis to be performed (i.e. `setup_storage` called beforehand).
pub async fn fund(pool: &ConnectionPool<Core>, addresses: &[Address]) {
    let mut storage = pool.connection().await.unwrap();

    let eth_amount = U256::from(10u32).pow(U256::from(32)); //10^32 wei

    for address in addresses {
        let key = storage_key_for_standard_token_balance(
            AccountTreeId::new(L2_BASE_TOKEN_ADDRESS),
            address,
        );
        let value = u256_to_h256(eth_amount);
        let storage_log = StorageLog::new_write_log(key, value);

        storage
            .storage_logs_dal()
            .append_storage_logs(L2BlockNumber(0), &[(H256::zero(), vec![storage_log])])
            .await
            .unwrap();
        if storage
            .storage_logs_dedup_dal()
            .filter_written_slots(&[storage_log.key.hashed_key()])
            .await
            .unwrap()
            .is_empty()
        {
            storage
                .storage_logs_dedup_dal()
                .insert_initial_writes(L1BatchNumber(0), &[storage_log.key])
                .await
                .unwrap();
        }
    }
}

pub(crate) const DEFAULT_GAS_PER_PUBDATA: u32 = 10000;

pub(crate) fn fee(gas_limit: u32) -> Fee {
    Fee {
        gas_limit: U256::from(gas_limit),
        max_fee_per_gas: SYSTEM_CONTEXT_MINIMAL_BASE_FEE.into(),
        max_priority_fee_per_gas: U256::zero(),
        gas_per_pubdata_limit: U256::from(DEFAULT_GAS_PER_PUBDATA),
    }
}

/// Returns a valid L2 transaction.
/// Automatically increments nonce of the account.
pub fn l2_transaction(account: &mut Account, gas_limit: u32) -> Transaction {
    account.get_l2_tx_for_execute(
        Execute {
            contract_address: Address::random(),
            calldata: vec![],
            value: Default::default(),
            factory_deps: vec![],
        },
        Some(fee(gas_limit)),
    )
}

pub fn l1_transaction(account: &mut Account, serial_id: PriorityOpId) -> Transaction {
    account.get_l1_tx(
        Execute {
            contract_address: Address::random(),
            value: Default::default(),
            calldata: vec![],
            factory_deps: vec![],
        },
        serial_id.0,
    )
}
