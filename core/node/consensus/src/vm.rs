use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _, scope};
use zksync_consensus_roles::attester;
use zksync_contracts::consensus as contracts;
use zksync_multivm::interface::TxExecutionMode;
use zksync_node_api_server::{
    execution_sandbox::{TransactionExecutor, TxExecutionArgs, TxSetupArgs, VmConcurrencyLimiter},
    tx_sender::MultiVMBaseSystemContracts,
};
use zksync_state::PostgresStorageCaches;
use zksync_system_constants::DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE;
use zksync_types::{
    ethabi, fee::Fee, fee_model::BatchFeeInput, l2::L2Tx, AccountTreeId, L2ChainId, Nonce, U256,
};
use zksync_vm_interface::ExecutionResult;

use crate::storage::ConnectionPool;

/// VM executes eth_calls on the db.
#[derive(Debug)]
pub(crate) struct VM {
    pool: ConnectionPool,
    setup_args: TxSetupArgs,
    limiter: VmConcurrencyLimiter,
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
                caches: PostgresStorageCaches::new(1, 1),
                validation_computational_gas_limit: u32::MAX,
                chain_id: L2ChainId::default(),
                whitelisted_tokens_for_aa: vec![],
                enforced_base_fee: None,
            },
            limiter: VmConcurrencyLimiter::new(1).0,
        }
    }

    pub async fn call<F: contracts::Function>(
        &self,
        ctx: &ctx::Ctx,
        batch: attester::BatchNumber,
        address: contracts::Address<F::Contract>,
        call: contracts::Call<F>,
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
        let permit = ctx.wait(self.limiter.acquire()).await?.unwrap();
        let mut conn = self.pool.connection(ctx).await.wrap("connection()")?;
        let args = conn
            .vm_block_args(ctx, batch)
            .await
            .wrap("vm_block_args()")?;
        let output = ctx
            .wait(TransactionExecutor::real(usize::MAX).execute_tx_in_sandbox(
                permit,
                self.setup_args.clone(),
                TxExecutionArgs::for_eth_call(tx.clone()),
                conn.0,
                args,
                None,
                vec![],
            ))
            .await?
            .context("execute_tx_in_sandbox()")?;
        match output.vm.result {
            ExecutionResult::Success { output } => {
                Ok(call.decode_outputs(&output).context("decode_output()")?)
            }
            other => Err(anyhow::format_err!("unsuccessful execution: {other:?}").into()),
        }
    }
}
