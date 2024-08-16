//! Implementation of "executing" methods, e.g. `eth_call`.

use async_trait::async_trait;
use zksync_dal::{Connection, Core};
use zksync_multivm::interface::{
    storage::ReadStorage, BytecodeCompressionError, OneshotEnv, TransactionExecutionMetrics,
    TxExecutionMode, VmExecutionResultAndLogs,
};
use zksync_types::{
    api::state_override::StateOverride, l2::L2Tx, transaction_request::CallOverrides,
    ExecuteTransactionCommon, Nonce, PackedEthSignature, Transaction, U256,
};

use super::{
    apply, storage::StorageWithOverrides, testonly::MockTransactionExecutor, vm_metrics, ApiTracer,
    BlockArgs, OneshotExecutor, TxSharedArgs, VmPermit,
};
use crate::execution_sandbox::apply::MainOneshotExecutor;

#[derive(Debug)]
pub(crate) struct TxExecutionArgs {
    pub execution_mode: TxExecutionMode,
    pub enforced_nonce: Option<Nonce>,
    pub added_balance: U256,
    pub enforced_base_fee: Option<u64>,
    pub missed_storage_invocation_limit: usize,
    /// If `true`, then the batch's L1/pubdata gas price will be adjusted so that the transaction's gas per pubdata limit is <=
    /// to the one in the block. This is often helpful in case we want the transaction validation to work regardless of the
    /// current L1 prices for gas or pubdata.
    pub adjust_pubdata_price: bool,
    pub transaction: Transaction,
}

impl TxExecutionArgs {
    pub fn for_validation(tx: L2Tx) -> Self {
        Self {
            execution_mode: TxExecutionMode::VerifyExecute,
            enforced_nonce: Some(tx.nonce()),
            added_balance: U256::zero(),
            enforced_base_fee: Some(tx.common_data.fee.max_fee_per_gas.as_u64()),
            missed_storage_invocation_limit: usize::MAX,
            adjust_pubdata_price: true,
            transaction: tx.into(),
        }
    }

    fn for_eth_call(
        enforced_base_fee: Option<u64>,
        vm_execution_cache_misses_limit: Option<usize>,
        mut call: L2Tx,
    ) -> Self {
        if call.common_data.signature.is_empty() {
            call.common_data.signature = PackedEthSignature::default().serialize_packed().into();
        }

        let missed_storage_invocation_limit = vm_execution_cache_misses_limit.unwrap_or(usize::MAX);
        Self {
            execution_mode: TxExecutionMode::EthCall,
            enforced_nonce: None,
            added_balance: U256::zero(),
            enforced_base_fee,
            missed_storage_invocation_limit,
            adjust_pubdata_price: false,
            transaction: call.into(),
        }
    }

    pub fn for_gas_estimate(
        vm_execution_cache_misses_limit: Option<usize>,
        transaction: Transaction,
        base_fee: u64,
    ) -> Self {
        let missed_storage_invocation_limit = vm_execution_cache_misses_limit.unwrap_or(usize::MAX);
        // For L2 transactions we need to explicitly put enough balance into the account of the users
        // while for L1->L2 transactions the `to_mint` field plays this role
        let added_balance = match &transaction.common_data {
            ExecuteTransactionCommon::L2(data) => data.fee.gas_limit * data.fee.max_fee_per_gas,
            ExecuteTransactionCommon::L1(_) => U256::zero(),
            ExecuteTransactionCommon::ProtocolUpgrade(_) => U256::zero(),
        };

        Self {
            execution_mode: TxExecutionMode::EstimateFee,
            missed_storage_invocation_limit,
            enforced_nonce: transaction.nonce(),
            added_balance,
            enforced_base_fee: Some(base_fee),
            adjust_pubdata_price: true,
            transaction,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TransactionExecutionOutput {
    /// Output of the VM.
    pub vm: VmExecutionResultAndLogs,
    /// Execution metrics.
    pub metrics: TransactionExecutionMetrics,
    /// Were published bytecodes OK?
    pub are_published_bytecodes_ok: bool,
}

/// Executor of transactions.
#[derive(Debug)]
pub(crate) enum TransactionExecutor {
    Real(MainOneshotExecutor),
    #[doc(hidden)] // Intended for tests only
    Mock(MockTransactionExecutor),
}

impl TransactionExecutor {
    pub fn real() -> Self {
        Self::Real(MainOneshotExecutor)
    }

    /// This method assumes that (block with number `resolved_block_number` is present in DB)
    /// or (`block_id` is `pending` and block with number `resolved_block_number - 1` is present in DB)
    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn execute_tx_in_sandbox(
        &self,
        vm_permit: VmPermit,
        shared_args: TxSharedArgs,
        execution_args: TxExecutionArgs,
        connection: Connection<'static, Core>,
        block_args: BlockArgs,
        state_override: Option<StateOverride>,
        tracers: Vec<ApiTracer>,
    ) -> anyhow::Result<TransactionExecutionOutput> {
        let total_factory_deps = execution_args.transaction.execute.factory_deps.len() as u16;
        let (env, storage) =
            apply::prepare_env_and_storage(connection, shared_args, &execution_args, &block_args)
                .await?;
        let state_override = state_override.unwrap_or_default();
        let storage = StorageWithOverrides::new(storage, &state_override);

        let (published_bytecodes, execution_result) = self
            .inspect_transaction_with_bytecode_compression(storage, env, execution_args, tracers)
            .await?;
        drop(vm_permit);

        let metrics =
            vm_metrics::collect_tx_execution_metrics(total_factory_deps, &execution_result);
        Ok(TransactionExecutionOutput {
            vm: execution_result,
            metrics,
            are_published_bytecodes_ok: published_bytecodes.is_ok(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute_tx_eth_call(
        &self,
        vm_permit: VmPermit,
        shared_args: TxSharedArgs,
        connection: Connection<'static, Core>,
        call_overrides: CallOverrides,
        tx: L2Tx,
        block_args: BlockArgs,
        vm_execution_cache_misses_limit: Option<usize>,
        custom_tracers: Vec<ApiTracer>,
        state_override: Option<StateOverride>,
    ) -> anyhow::Result<VmExecutionResultAndLogs> {
        let execution_args = TxExecutionArgs::for_eth_call(
            call_overrides.enforced_base_fee,
            vm_execution_cache_misses_limit,
            tx,
        );

        let output = self
            .execute_tx_in_sandbox(
                vm_permit,
                shared_args,
                execution_args,
                connection,
                block_args,
                state_override,
                custom_tracers,
            )
            .await?;
        Ok(output.vm)
    }
}

#[async_trait]
impl<S> OneshotExecutor<S> for TransactionExecutor
where
    S: ReadStorage + Send + 'static,
{
    type Tracers = Vec<ApiTracer>;

    async fn inspect_transaction(
        &self,
        storage: S,
        env: OneshotEnv,
        args: TxExecutionArgs,
        tracers: Self::Tracers,
    ) -> anyhow::Result<VmExecutionResultAndLogs> {
        match self {
            Self::Real(executor) => {
                executor
                    .inspect_transaction(storage, env, args, tracers)
                    .await
            }
            Self::Mock(executor) => executor.inspect_transaction(storage, env, args, ()).await,
        }
    }

    async fn inspect_transaction_with_bytecode_compression(
        &self,
        storage: S,
        env: OneshotEnv,
        args: TxExecutionArgs,
        tracers: Self::Tracers,
    ) -> anyhow::Result<(
        Result<(), BytecodeCompressionError>,
        VmExecutionResultAndLogs,
    )> {
        match self {
            Self::Real(executor) => {
                executor
                    .inspect_transaction_with_bytecode_compression(storage, env, args, tracers)
                    .await
            }
            Self::Mock(executor) => {
                executor
                    .inspect_transaction_with_bytecode_compression(storage, env, args, ())
                    .await
            }
        }
    }
}
