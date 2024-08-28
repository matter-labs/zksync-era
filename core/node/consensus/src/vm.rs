use anyhow::Context as _;
use zksync_concurrency::{ctx, scope, error::Wrap as _};
use zksync_contracts::consensus as contracts;
use zksync_consensus_roles::{attester};
use zksync_node_api_server::{
    execution_sandbox::{VmConcurrencyLimiter,TxSharedArgs},
    tx_sender::{MultiVMBaseSystemContracts}};
use zksync_system_constants::DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE;
use zksync_vm_interface::ExecutionResult;
use zksync_node_api_server::execution_sandbox::TransactionExecutor;
use zksync_state::PostgresStorageCaches;
use zksync_types::{
    L2ChainId,
    AccountTreeId,
    ethabi,
    fee_model::BatchFeeInput,
    fee::Fee,
    l2::L2Tx,
    transaction_request::CallOverrides,
    Nonce, U256,
};
use crate::storage::{ConnectionPool};

/// VM executes eth_calls on the db.
#[derive(Debug)]
pub(crate) struct VM {
    pool: ConnectionPool,
    tx_shared_args: TxSharedArgs,
    limiter: VmConcurrencyLimiter,
}

impl VM {
    /// Constructs a new `VMReader` instance.
    pub async fn new(pool: ConnectionPool) -> Self {
        Self {
            pool,
            tx_shared_args: TxSharedArgs {
                operator_account: AccountTreeId::default(),
                fee_input: BatchFeeInput::sensible_l1_pegged_default(),
                base_system_contracts: scope::wait_blocking(MultiVMBaseSystemContracts::load_eth_call_blocking).await,
                caches: PostgresStorageCaches::new(1, 1),
                validation_computational_gas_limit: u32::MAX,
                chain_id: L2ChainId::default(),
                whitelisted_tokens_for_aa: vec![],
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
        let args = self.pool.connection(ctx).await.wrap("connection()")?.block_args(ctx,batch).await.wrap("block_args()")?;
        let permit = ctx.wait(self.limiter.acquire()).await?.unwrap();
        let output = ctx.wait(TransactionExecutor::Real.execute_tx_eth_call(
            permit,
            self.tx_shared_args.clone(), 
            self.pool.0.clone(),
            CallOverrides { enforced_base_fee: None },
            tx,
            args,
            None,
            vec![],
            None,
        )).await?.context("execute_tx_eth_call()")?;
        match output.result {
            ExecutionResult::Success { output } => Ok(call.decode_outputs(&output).context("decode_output()")?),
            other => Err(anyhow::format_err!("unsuccessful execution: {other:?}").into()),
        }
    }
}
