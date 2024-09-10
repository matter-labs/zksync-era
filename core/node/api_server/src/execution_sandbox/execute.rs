//! Implementation of "executing" methods, e.g. `eth_call`.

use async_trait::async_trait;
use zksync_dal::{Connection, Core};
use zksync_multivm::interface::{
    storage::ReadStorage, BytecodeCompressionError, OneshotEnv, TransactionExecutionMetrics,
    VmExecutionResultAndLogs,
};
use zksync_types::{
    api::state_override::StateOverride, l2::L2Tx, ExecuteTransactionCommon, Nonce,
    PackedEthSignature, Transaction, U256,
};

use super::{
    apply::{self, MainOneshotExecutor},
    storage::StorageWithOverrides,
    testonly::MockOneshotExecutor,
    vm_metrics, ApiTracer, BlockArgs, OneshotExecutor, TxSetupArgs, VmPermit,
};

/// Executor-independent arguments necessary to for oneshot transaction execution.
///
/// # Developer guidelines
///
/// Please don't add fields that duplicate `SystemEnv` or `L1BatchEnv` information, since both of these
/// are also provided to an executor.
#[derive(Debug)]
pub struct TxExecutionArgs {
    /// Transaction / call itself.
    pub transaction: Transaction,
    /// Nonce override for the initiator account.
    pub enforced_nonce: Option<Nonce>,
    /// Balance added to the initiator account.
    pub added_balance: U256,
    /// If `true`, then the batch's L1 / pubdata gas price will be adjusted so that the transaction's gas per pubdata limit is <=
    /// to the one in the block. This is often helpful in case we want the transaction validation to work regardless of the
    /// current L1 prices for gas or pubdata.
    pub adjust_pubdata_price: bool,
}

impl TxExecutionArgs {
    pub fn for_validation(tx: L2Tx) -> Self {
        Self {
            enforced_nonce: Some(tx.nonce()),
            added_balance: U256::zero(),
            adjust_pubdata_price: true,
            transaction: tx.into(),
        }
    }

    pub fn for_eth_call(mut call: L2Tx) -> Self {
        if call.common_data.signature.is_empty() {
            call.common_data.signature = PackedEthSignature::default().serialize_packed().into();
        }

        Self {
            enforced_nonce: None,
            added_balance: U256::zero(),
            adjust_pubdata_price: false,
            transaction: call.into(),
        }
    }

    pub fn for_gas_estimate(transaction: Transaction) -> Self {
        // For L2 transactions we need to explicitly put enough balance into the account of the users
        // while for L1->L2 transactions the `to_mint` field plays this role
        let added_balance = match &transaction.common_data {
            ExecuteTransactionCommon::L2(data) => data.fee.gas_limit * data.fee.max_fee_per_gas,
            ExecuteTransactionCommon::L1(_) => U256::zero(),
            ExecuteTransactionCommon::ProtocolUpgrade(_) => U256::zero(),
        };

        Self {
            enforced_nonce: transaction.nonce(),
            added_balance,
            adjust_pubdata_price: true,
            transaction,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TransactionExecutionOutput {
    /// Output of the VM.
    pub vm: VmExecutionResultAndLogs,
    /// Execution metrics.
    pub metrics: TransactionExecutionMetrics,
    /// Were published bytecodes OK?
    pub are_published_bytecodes_ok: bool,
}

/// Executor of transactions.
#[derive(Debug)]
pub enum TransactionExecutor {
    Real(MainOneshotExecutor),
    #[doc(hidden)] // Intended for tests only
    Mock(MockOneshotExecutor),
}

impl TransactionExecutor {
    pub fn real(missed_storage_invocation_limit: usize) -> Self {
        Self::Real(MainOneshotExecutor::new(missed_storage_invocation_limit))
    }

    /// This method assumes that (block with number `resolved_block_number` is present in DB)
    /// or (`block_id` is `pending` and block with number `resolved_block_number - 1` is present in DB)
    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn execute_tx_in_sandbox(
        &self,
        vm_permit: VmPermit,
        setup_args: TxSetupArgs,
        execution_args: TxExecutionArgs,
        connection: Connection<'static, Core>,
        block_args: BlockArgs,
        state_override: Option<StateOverride>,
        tracers: Vec<ApiTracer>,
    ) -> anyhow::Result<TransactionExecutionOutput> {
        let total_factory_deps = execution_args.transaction.execute.factory_deps.len() as u16;
        let (env, storage) =
            apply::prepare_env_and_storage(connection, setup_args, &block_args).await?;
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
