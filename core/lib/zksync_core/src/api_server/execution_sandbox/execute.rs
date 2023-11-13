//! Implementation of "executing" methods, e.g. `eth_call`.

use tracing::{span, Level};

use multivm::interface::{TxExecutionMode, VmExecutionMode, VmExecutionResultAndLogs, VmInterface};
use multivm::tracers::StorageInvocations;
use multivm::vm_latest::constants::ETH_CALL_GAS_LIMIT;
use multivm::MultivmTracer;
use zksync_dal::ConnectionPool;

use zksync_types::{
    fee::TransactionExecutionMetrics, l2::L2Tx, ExecuteTransactionCommon, Nonce,
    PackedEthSignature, Transaction, U256,
};

use super::{apply, vm_metrics, ApiTracer, BlockArgs, TxSharedArgs, VmPermit};

#[derive(Debug)]
pub(crate) struct TxExecutionArgs {
    pub execution_mode: TxExecutionMode,
    pub enforced_nonce: Option<Nonce>,
    pub added_balance: U256,
    pub enforced_base_fee: Option<u64>,
    pub missed_storage_invocation_limit: usize,
}

impl TxExecutionArgs {
    pub fn for_validation(tx: &L2Tx) -> Self {
        Self {
            execution_mode: TxExecutionMode::VerifyExecute,
            enforced_nonce: Some(tx.nonce()),
            added_balance: U256::zero(),
            enforced_base_fee: Some(tx.common_data.fee.max_fee_per_gas.as_u64()),
            missed_storage_invocation_limit: usize::MAX,
        }
    }

    fn for_eth_call(
        enforced_base_fee: u64,
        vm_execution_cache_misses_limit: Option<usize>,
    ) -> Self {
        let missed_storage_invocation_limit = vm_execution_cache_misses_limit.unwrap_or(usize::MAX);
        Self {
            execution_mode: TxExecutionMode::EthCall,
            enforced_nonce: None,
            added_balance: U256::zero(),
            enforced_base_fee: Some(enforced_base_fee),
            missed_storage_invocation_limit,
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
            execution_mode: TxExecutionMode::EstimateFee,
            missed_storage_invocation_limit,
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
    custom_tracers: Vec<ApiTracer>,
) -> VmExecutionResultAndLogs {
    let enforced_base_fee = tx.common_data.fee.max_fee_per_gas.as_u64();
    let execution_args =
        TxExecutionArgs::for_eth_call(enforced_base_fee, vm_execution_cache_misses_limit);

    if tx.common_data.signature.is_empty() {
        tx.common_data.signature = PackedEthSignature::default().serialize_packed().into();
    }

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
        custom_tracers,
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
) -> (VmExecutionResultAndLogs, TransactionExecutionMetrics) {
    let mut connection = connection_pool.access_storage_tagged("api").await.unwrap();
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
        vec![],
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
    custom_tracers: Vec<ApiTracer>,
) -> (VmExecutionResultAndLogs, TransactionExecutionMetrics) {
    let total_factory_deps = tx
        .execute
        .factory_deps
        .as_ref()
        .map_or(0, |deps| deps.len() as u16);

    let execution_result = tokio::task::spawn_blocking(move || {
        let span = span!(Level::DEBUG, "execute_in_sandbox").entered();
        let result = apply::apply_vm_in_sandbox(
            vm_permit,
            shared_args,
            &execution_args,
            &connection_pool,
            tx,
            block_args,
            |vm, tx| {
                vm.push_transaction(tx);
                let storage_invocation_tracer =
                    StorageInvocations::new(execution_args.missed_storage_invocation_limit);
                let custom_tracers: Vec<_> = custom_tracers
                    .into_iter()
                    .map(|tracer| tracer.into_boxed())
                    .chain(vec![storage_invocation_tracer.into_tracer_pointer()])
                    .collect();
                vm.inspect(custom_tracers.into(), VmExecutionMode::OneTx)
            },
        );
        span.exit();
        result
    })
    .await
    .unwrap();

    let tx_execution_metrics =
        vm_metrics::collect_tx_execution_metrics(total_factory_deps, &execution_result);
    (execution_result, tx_execution_metrics)
}
