//! Test utilities that can be used for testing sequencer that may
//! be useful outside of this crate.

use async_trait::async_trait;
use once_cell::sync::Lazy;
use zksync_contracts::BaseSystemContracts;
use zksync_dal::{ConnectionPool, Core, CoreDal as _};
use zksync_multivm::interface::{
    executor::{BatchExecutor, BatchExecutorFactory},
    storage::{InMemoryStorage, StorageView},
    BatchTransactionExecutionResult, ExecutionResult, FinishedL1Batch, L1BatchEnv, L2BlockEnv,
    SystemEnv, VmExecutionResultAndLogs,
};
use zksync_state::OwnedStorage;
use zksync_test_account::Account;
use zksync_types::{
    fee::Fee, utils::storage_key_for_standard_token_balance, AccountTreeId, Address, Execute,
    L1BatchNumber, L2BlockNumber, PriorityOpId, StorageLog, Transaction, L2_BASE_TOKEN_ADDRESS,
    SYSTEM_CONTEXT_MINIMAL_BASE_FEE, U256,
};
use zksync_utils::u256_to_h256;

pub mod test_batch_executor;

pub(super) static BASE_SYSTEM_CONTRACTS: Lazy<BaseSystemContracts> =
    Lazy::new(BaseSystemContracts::load_from_disk);

/// Creates a `TxExecutionResult` object denoting a successful tx execution.
pub(crate) fn successful_exec() -> BatchTransactionExecutionResult {
    BatchTransactionExecutionResult {
        tx_result: Box::new(VmExecutionResultAndLogs {
            result: ExecutionResult::Success { output: vec![] },
            logs: Default::default(),
            statistics: Default::default(),
            refunds: Default::default(),
        }),
        compressed_bytecodes: vec![],
        call_traces: vec![],
    }
}

/// `BatchExecutor` which doesn't check anything at all. Accepts all transactions.
#[derive(Debug)]
pub struct MockBatchExecutor;

impl BatchExecutorFactory<OwnedStorage> for MockBatchExecutor {
    fn init_batch(
        &mut self,
        _storage: OwnedStorage,
        _l1_batch_env: L1BatchEnv,
        _system_env: SystemEnv,
    ) -> Box<dyn BatchExecutor<OwnedStorage>> {
        Box::new(Self)
    }
}

#[async_trait]
impl BatchExecutor<OwnedStorage> for MockBatchExecutor {
    async fn execute_tx(
        &mut self,
        _tx: Transaction,
    ) -> anyhow::Result<BatchTransactionExecutionResult> {
        Ok(successful_exec())
    }

    async fn rollback_last_tx(&mut self) -> anyhow::Result<()> {
        panic!("unexpected rollback");
    }

    async fn start_next_l2_block(&mut self, _env: L2BlockEnv) -> anyhow::Result<()> {
        Ok(())
    }

    async fn finish_batch(
        self: Box<Self>,
    ) -> anyhow::Result<(FinishedL1Batch, StorageView<OwnedStorage>)> {
        let storage = OwnedStorage::boxed(InMemoryStorage::default());
        Ok((FinishedL1Batch::mock(), StorageView::new(storage)))
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
            .append_storage_logs(L2BlockNumber(0), &[storage_log])
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
                .insert_initial_writes(L1BatchNumber(0), &[storage_log.key.hashed_key()])
                .await
                .unwrap();
        }
    }
}

pub(crate) const DEFAULT_GAS_PER_PUBDATA: u32 = 10000;

pub fn fee(gas_limit: u32) -> Fee {
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
            contract_address: Some(Address::random()),
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
            contract_address: Some(Address::random()),
            value: Default::default(),
            calldata: vec![],
            factory_deps: vec![],
        },
        serial_id.0,
    )
}
