//! Implementation of "executing" methods, e.g. `eth_call`.

use tracing::{span, Level};

use std::{collections::HashMap, mem};

use vm::{
    utils::ETH_CALL_GAS_LIMIT,
    vm_with_bootloader::{
        push_transaction_to_bootloader_memory, BootloaderJobType, TxExecutionMode,
    },
    VmExecutionResult,
};
use zksync_dal::ConnectionPool;
use zksync_types::{
    fee::TransactionExecutionMetrics, l2::L2Tx, ExecuteTransactionCommon, Nonce, StorageKey,
    Transaction, H256, U256,
};

use super::{apply, error::SandboxExecutionError, vm_metrics, BlockArgs, TxSharedArgs, VmPermit};

#[derive(Debug)]
pub(crate) struct TxExecutionArgs {
    pub execution_mode: TxExecutionMode,
    pub enforced_nonce: Option<Nonce>,
    pub added_balance: U256,
    pub enforced_base_fee: Option<u64>,
}

impl TxExecutionArgs {
    pub fn for_validation(tx: &L2Tx) -> Self {
        Self {
            execution_mode: TxExecutionMode::VerifyExecute,
            enforced_nonce: Some(tx.nonce()),
            added_balance: U256::zero(),
            enforced_base_fee: Some(tx.common_data.fee.max_fee_per_gas.as_u64()),
        }
    }

    fn for_eth_call(
        enforced_base_fee: u64,
        vm_execution_cache_misses_limit: Option<usize>,
    ) -> Self {
        let missed_storage_invocation_limit = vm_execution_cache_misses_limit.unwrap_or(usize::MAX);
        Self {
            execution_mode: TxExecutionMode::EthCall {
                missed_storage_invocation_limit,
            },
            enforced_nonce: None,
            added_balance: U256::zero(),
            enforced_base_fee: Some(enforced_base_fee),
        }
    }

    pub fn for_gas_estimate(
        vm_execution_cache_misses_limit: Option<usize>,
        tx: &Transaction,
        base_fee: u64,
    ) -> Self {
        let missed_storage_invocation_limit = vm_execution_cache_misses_limit.unwrap_or(usize::MAX);
        // For L2 transactions we need to explicitly put enough balance into the account of the users
        // while for L1->L2 transactions the `to_mint` field plays this role
        let added_balance = match &tx.common_data {
            ExecuteTransactionCommon::L2(data) => data.fee.gas_limit * data.fee.max_fee_per_gas,
            ExecuteTransactionCommon::L1(_) => U256::zero(),
            ExecuteTransactionCommon::ProtocolUpgrade(_) => U256::zero(),
        };

        Self {
            execution_mode: TxExecutionMode::EstimateFee {
                missed_storage_invocation_limit,
            },
            enforced_nonce: tx.nonce(),
            added_balance,
            enforced_base_fee: Some(base_fee),
        }
    }
}

pub(crate) async fn execute_tx_eth_call(
    vm_permit: VmPermit,
    shared_args: TxSharedArgs,
    connection_pool: ConnectionPool,
    mut tx: L2Tx,
    block_args: BlockArgs,
    vm_execution_cache_misses_limit: Option<usize>,
    trace_call: bool,
) -> Result<VmExecutionResult, SandboxExecutionError> {
    let enforced_base_fee = tx.common_data.fee.max_fee_per_gas.as_u64();
    let execution_args =
        TxExecutionArgs::for_eth_call(enforced_base_fee, vm_execution_cache_misses_limit);

    // Protection against infinite-loop eth_calls and alike:
    // limiting the amount of gas the call can use.
    // We can't use BLOCK_ERGS_LIMIT here since the VM itself has some overhead.
    tx.common_data.fee.gas_limit = ETH_CALL_GAS_LIMIT.into();
    let (vm_result, _) = execute_tx_in_sandbox(
        vm_permit,
        shared_args,
        execution_args,
        connection_pool,
        tx.into(),
        block_args,
        BootloaderJobType::TransactionExecution,
        trace_call,
        &mut HashMap::new(),
    )
    .await;

    vm_result
}

#[tracing::instrument(skip_all)]
pub(crate) async fn execute_tx_with_pending_state(
    vm_permit: VmPermit,
    mut shared_args: TxSharedArgs,
    execution_args: TxExecutionArgs,
    connection_pool: ConnectionPool,
    tx: Transaction,
    storage_read_cache: &mut HashMap<StorageKey, H256>,
) -> (
    Result<VmExecutionResult, SandboxExecutionError>,
    TransactionExecutionMetrics,
) {
    let mut connection = connection_pool.access_storage_tagged("api").await;
    let block_args = BlockArgs::pending(&mut connection).await;
    drop(connection);
    // In order for execution to pass smoothlessly, we need to ensure that block's required gasPerPubdata will be
    // <= to the one in the transaction itself.
    shared_args.adjust_l1_gas_price(tx.gas_per_pubdata_byte_limit());

    execute_tx_in_sandbox(
        vm_permit,
        shared_args,
        execution_args,
        connection_pool,
        tx,
        block_args,
        BootloaderJobType::TransactionExecution,
        false,
        storage_read_cache,
    )
    .await
}

/// This method assumes that (block with number `resolved_block_number` is present in DB)
/// or (`block_id` is `pending` and block with number `resolved_block_number - 1` is present in DB)
#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all)]
async fn execute_tx_in_sandbox(
    vm_permit: VmPermit,
    shared_args: TxSharedArgs,
    execution_args: TxExecutionArgs,
    connection_pool: ConnectionPool,
    tx: Transaction,
    block_args: BlockArgs,
    job_type: BootloaderJobType,
    trace_call: bool,
    storage_read_cache: &mut HashMap<StorageKey, H256>,
) -> (
    Result<VmExecutionResult, SandboxExecutionError>,
    TransactionExecutionMetrics,
) {
    let total_factory_deps = tx
        .execute
        .factory_deps
        .as_ref()
        .map_or(0, |deps| deps.len() as u16);

    let moved_cache = mem::take(storage_read_cache);
    let (execution_result, moved_cache) = tokio::task::spawn_blocking(move || {
        let span = span!(Level::DEBUG, "execute_in_sandbox").entered();
        let execution_mode = execution_args.execution_mode;
        let result = apply::apply_vm_in_sandbox(
            vm_permit,
            shared_args,
            &execution_args,
            &connection_pool,
            tx,
            block_args,
            moved_cache,
            |vm, tx| {
                push_transaction_to_bootloader_memory(vm, &tx, execution_mode, None);
                let result = if trace_call {
                    vm.execute_till_block_end_with_call_tracer(job_type)
                } else {
                    vm.execute_till_block_end(job_type)
                };
                result.full_result
            },
        );
        span.exit();
        result
    })
    .await
    .unwrap();

    *storage_read_cache = moved_cache;

    let tx_execution_metrics =
        vm_metrics::collect_tx_execution_metrics(total_factory_deps, &execution_result);
    let result = match execution_result.revert_reason {
        None => Ok(execution_result),
        Some(revert) => Err(revert.revert_reason.into()),
    };
    (result, tx_execution_metrics)
}
