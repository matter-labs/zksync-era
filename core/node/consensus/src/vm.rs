use std::sync::Arc;

use anyhow::Context as _;
use tokio::runtime::Handle;
use zksync_concurrency::{ctx, error::Wrap as _, scope};
use zksync_consensus_roles::validator;
use zksync_state::PostgresStorage;
use zksync_system_constants::DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE;
use zksync_types::{ethabi, fee::Fee, l2::L2Tx, AccountTreeId, L2ChainId, Nonce, U256};
use zksync_vm_executor::oneshot::{
    CallOrExecute, MainOneshotExecutor, MultiVmBaseSystemContracts, OneshotEnvParameters,
};
use zksync_vm_interface::{
    executor::OneshotExecutor, storage::StorageWithOverrides, ExecutionResult,
    OneshotTracingParams, TxExecutionArgs, VmRevertReason,
};

use crate::{abi, storage::ConnectionPool};

/// Represents the result of a VM execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VMResult<T> {
    /// VM execution completed successfully with decoded output.
    Success { output: T },
    /// VM execution was reverted.
    Revert { revert_reason: String },
}

/// VM executes eth_calls on the db.
#[derive(Debug)]
pub(crate) struct VM {
    pool: ConnectionPool,
    options: OneshotEnvParameters<CallOrExecute>,
    executor: MainOneshotExecutor,
}

impl VM {
    /// Constructs a new `VM` instance.
    pub async fn new(pool: ConnectionPool) -> Self {
        let base_system_contracts =
            scope::wait_blocking(MultiVmBaseSystemContracts::load_eth_call_blocking).await;
        Self {
            pool,
            // L2 chain ID and fee account don't seem to matter for calls, hence the use of default values.
            options: OneshotEnvParameters::new(
                Arc::new(base_system_contracts),
                L2ChainId::default(),
                AccountTreeId::default(),
                u32::MAX,
            ),
            executor: MainOneshotExecutor::new(usize::MAX),
        }
    }

    pub async fn call<F: abi::Function>(
        &self,
        ctx: &ctx::Ctx,
        block_number: validator::BlockNumber,
        address: abi::Address<F::Contract>,
        call: abi::Call<F>,
    ) -> ctx::Result<VMResult<F::Outputs>> {
        let tx = L2Tx::new(
            Some(*address),
            call.calldata().context("call.calldata()")?,
            Nonce(0),
            Fee {
                gas_limit: U256::from(2000000000u32),
                max_fee_per_gas: U256::zero(),
                max_priority_fee_per_gas: U256::zero(),
                gas_per_pubdata_limit: U256::from(DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE),
            },
            ethabi::Address::zero(),
            U256::zero(),
            vec![],
            Default::default(),
        );

        let mut conn = self.pool.connection(ctx).await.wrap("connection()")?;
        let (block_info, fee_input) = conn
            .vm_block_info(ctx, block_number)
            .await
            .wrap("vm_block_info()")?;

        let env = ctx
            .wait(
                self.options
                    .to_call_env(&mut conn.0, &block_info, fee_input, None),
            )
            .await?
            .context("to_env()")?;

        let storage = ctx
            .wait(PostgresStorage::new_async(
                Handle::current(),
                conn.0,
                block_info.state_l2_block_number(),
                false,
            ))
            .await?
            .context("PostgresStorage")?;

        let output = ctx
            .wait(self.executor.inspect_transaction_with_bytecode_compression(
                StorageWithOverrides::new(storage),
                env,
                TxExecutionArgs::for_eth_call(tx),
                OneshotTracingParams::default(),
            ))
            .await?
            .context("execute_tx_in_sandbox()")?;
        match output.tx_result.result {
            // If the execution was successful, we can decode the output.
            ExecutionResult::Success { output } => {
                let output = call.decode_outputs(&output).context("decode_output()")?;
                Ok(VMResult::Success { output })
            }
            // If the execution was reverted by a general revert reason (i.e. the contract called revert()),
            // we can extract the revert reason from the output.
            // We ignore the data from the revert reason, but we can decode it if necessary.
            ExecutionResult::Revert {
                output: VmRevertReason::General { msg, .. },
            } => Ok(VMResult::Revert { revert_reason: msg }),
            // In all other cases, we just return the error.
            other => Err(anyhow::format_err!("Unsuccessful execution: {other:?}").into()),
        }
    }
}
