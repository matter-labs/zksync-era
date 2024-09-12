use anyhow::Context as _;
use tokio::runtime::Handle;
use zksync_concurrency::{ctx, error::Wrap as _, scope};
use zksync_consensus_roles::attester;
use zksync_state::PostgresStorage;
use zksync_system_constants::DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE;
use zksync_types::{
    ethabi, fee::Fee, fee_model::BatchFeeInput, l2::L2Tx, AccountTreeId, L2ChainId, Nonce, U256,
};
use zksync_vm_executor::oneshot::{MainOneshotExecutor, MultiVMBaseSystemContracts, TxSetupArgs};
use zksync_vm_interface::{
    executor::OneshotExecutor, ExecutionResult, OneshotTracingParams, TxExecutionArgs,
    TxExecutionMode,
};

use crate::{abi, storage::ConnectionPool};

/// VM executes eth_calls on the db.
#[derive(Debug)]
pub(crate) struct VM {
    pool: ConnectionPool,
    setup_args: TxSetupArgs,
    executor: MainOneshotExecutor,
}

impl VM {
    /// Constructs a new `VM` instance.
    pub async fn new(pool: ConnectionPool) -> Self {
        Self {
            pool,
            setup_args: TxSetupArgs {
                execution_mode: TxExecutionMode::EthCall,
                operator_account: AccountTreeId::default(),
                fee_input: BatchFeeInput::sensible_l1_pegged_default(),
                base_system_contracts: scope::wait_blocking(
                    MultiVMBaseSystemContracts::load_eth_call_blocking,
                )
                .await,
                validation_computational_gas_limit: u32::MAX,
                chain_id: L2ChainId::default(),
                enforced_base_fee: None,
            },
            executor: MainOneshotExecutor::new(usize::MAX),
        }
    }

    pub async fn call<F: abi::Function>(
        &self,
        ctx: &ctx::Ctx,
        batch: attester::BatchNumber,
        address: abi::Address<F::Contract>,
        call: abi::Call<F>,
    ) -> ctx::Result<F::Outputs> {
        let tx = L2Tx::new(
            *address,
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
        let block_info = conn
            .vm_block_info(ctx, batch)
            .await
            .wrap("vm_block_info()")?;
        let env = ctx
            .wait(self.setup_args.to_env(&mut conn.0, &block_info))
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
                storage,
                env,
                TxExecutionArgs::for_eth_call(tx),
                OneshotTracingParams::default(),
            ))
            .await?
            .context("execute_tx_in_sandbox()")?;
        match output.tx_result.result {
            ExecutionResult::Success { output } => {
                Ok(call.decode_outputs(&output).context("decode_output()")?)
            }
            other => Err(anyhow::format_err!("unsuccessful execution: {other:?}").into()),
        }
    }
}
