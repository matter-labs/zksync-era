use std::sync::Arc;

use anyhow::Context as _;
use once_cell::sync::OnceCell;
use zksync_dal::{CoreDal, DalError};
use zksync_multivm::{
    interface::ExecutionResult, vm_latest::constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
};
use zksync_system_constants::MAX_ENCODED_TX_SIZE;
use zksync_types::{
    api::{BlockId, BlockNumber, DebugCall, ResultDebugCall, TracerConfig},
    debug_flat_call::{flatten_debug_calls, DebugCallFlat},
    fee_model::BatchFeeInput,
    l2::L2Tx,
    transaction_request::CallRequest,
    vm_trace::Call,
    AccountTreeId, H256,
};
use zksync_web3_decl::error::Web3Error;

use crate::{
    execution_sandbox::{ApiTracer, TxSharedArgs},
    tx_sender::{ApiContracts, TxSenderConfig},
    web3::{backend_jsonrpsee::MethodTracer, state::RpcState},
};

#[derive(Debug, Clone)]
pub(crate) struct DebugNamespace {
    batch_fee_input: BatchFeeInput,
    state: RpcState,
    api_contracts: ApiContracts,
}

impl DebugNamespace {
    pub async fn new(state: RpcState) -> anyhow::Result<Self> {
        let api_contracts = ApiContracts::load_from_disk().await?;
        let fee_input_provider = &state.tx_sender.0.batch_fee_input_provider;
        let batch_fee_input = fee_input_provider
            .get_batch_fee_input_scaled(
                state.api_config.estimate_gas_scale_factor,
                state.api_config.estimate_gas_scale_factor,
            )
            .await
            .context("cannot get batch fee input")?;

        Ok(Self {
            // For now, the same scaling is used for both the L1 gas price and the pubdata price
            batch_fee_input,
            state,
            api_contracts,
        })
    }

    fn sender_config(&self) -> &TxSenderConfig {
        &self.state.tx_sender.0.sender_config
    }

    pub(crate) fn current_method(&self) -> &MethodTracer {
        &self.state.current_method
    }

    pub async fn debug_trace_block_impl(
        &self,
        block_id: BlockId,
        options: Option<TracerConfig>,
    ) -> Result<Vec<ResultDebugCall>, Web3Error> {
        self.current_method().set_block_id(block_id);
        if matches!(block_id, BlockId::Number(BlockNumber::Pending)) {
            // See `EthNamespace::get_block_impl()` for an explanation why this check is needed.
            return Ok(vec![]);
        }

        let only_top_call = options
            .map(|options| options.tracer_config.only_top_call)
            .unwrap_or(false);
        let mut connection = self.state.acquire_connection().await?;
        let block_number = self.state.resolve_block(&mut connection, block_id).await?;
        self.current_method()
            .set_block_diff(self.state.last_sealed_l2_block.diff(block_number));

        let call_traces = connection
            .blocks_web3_dal()
            .get_traces_for_l2_block(block_number)
            .await
            .map_err(DalError::generalize)?;
        let call_trace = call_traces
            .into_iter()
            .map(|call_trace| {
                let mut result: DebugCall = call_trace.into();
                if only_top_call {
                    result.calls = vec![];
                }
                ResultDebugCall { result }
            })
            .collect();
        Ok(call_trace)
    }

    pub async fn debug_trace_block_flat_impl(
        &self,
        block_id: BlockId,
        options: Option<TracerConfig>,
    ) -> Result<Vec<DebugCallFlat>, Web3Error> {
        let call_trace = self.debug_trace_block_impl(block_id, options).await?;
        let call_trace_flat = flatten_debug_calls(call_trace);
        Ok(call_trace_flat)
    }

    pub async fn debug_trace_transaction_impl(
        &self,
        tx_hash: H256,
        options: Option<TracerConfig>,
    ) -> Result<Option<DebugCall>, Web3Error> {
        let only_top_call = options
            .map(|options| options.tracer_config.only_top_call)
            .unwrap_or(false);
        let mut connection = self.state.acquire_connection().await?;
        let call_trace = connection
            .transactions_dal()
            .get_call_trace(tx_hash)
            .await
            .map_err(DalError::generalize)?;
        Ok(call_trace.map(|call_trace| {
            let mut result: DebugCall = call_trace.into();
            if only_top_call {
                result.calls = vec![];
            }
            result
        }))
    }

    pub async fn debug_trace_call_impl(
        &self,
        mut request: CallRequest,
        block_id: Option<BlockId>,
        options: Option<TracerConfig>,
    ) -> Result<DebugCall, Web3Error> {
        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumber::Pending));
        self.current_method().set_block_id(block_id);

        let only_top_call = options
            .map(|options| options.tracer_config.only_top_call)
            .unwrap_or(false);

        let mut connection = self.state.acquire_connection().await?;
        let block_args = self
            .state
            .resolve_block_args(&mut connection, block_id)
            .await?;
        drop(connection);

        self.current_method().set_block_diff(
            self.state
                .last_sealed_l2_block
                .diff_with_block_args(&block_args),
        );

        if request.gas.is_none() {
            request.gas = Some(
                self.state
                    .tx_sender
                    .get_default_eth_call_gas(block_args)
                    .await
                    .map_err(Web3Error::InternalError)?
                    .into(),
            )
        }

        let call_overrides = request.get_call_overrides()?;
        let tx = L2Tx::from_request(request.into(), MAX_ENCODED_TX_SIZE)?;

        let shared_args = self.shared_args().await;
        let vm_permit = self
            .state
            .tx_sender
            .vm_concurrency_limiter()
            .acquire()
            .await;
        let vm_permit = vm_permit.context("cannot acquire VM permit")?;

        // We don't need properly trace if we only need top call
        let call_tracer_result = Arc::new(OnceCell::default());
        let custom_tracers = if only_top_call {
            vec![]
        } else {
            vec![ApiTracer::CallTracer(call_tracer_result.clone())]
        };

        let executor = &self.state.tx_sender.0.executor;
        let result = executor
            .execute_tx_eth_call(
                vm_permit,
                shared_args,
                self.state.connection_pool.clone(),
                call_overrides,
                tx.clone(),
                block_args,
                self.sender_config().vm_execution_cache_misses_limit,
                custom_tracers,
            )
            .await?;

        let (output, revert_reason) = match result.result {
            ExecutionResult::Success { output, .. } => (output, None),
            ExecutionResult::Revert { output } => (vec![], Some(output.to_string())),
            ExecutionResult::Halt { reason } => {
                return Err(Web3Error::SubmitTransactionError(
                    reason.to_string(),
                    vec![],
                ))
            }
        };

        // We had only one copy of Arc this arc is already dropped it's safe to unwrap
        let trace = Arc::try_unwrap(call_tracer_result)
            .unwrap()
            .take()
            .unwrap_or_default();
        let call = Call::new_high_level(
            tx.common_data.fee.gas_limit.as_u64(),
            result.statistics.gas_used,
            tx.execute.value,
            tx.execute.calldata,
            output,
            revert_reason,
            trace,
        );
        Ok(call.into())
    }

    async fn shared_args(&self) -> TxSharedArgs {
        let sender_config = self.sender_config();
        TxSharedArgs {
            operator_account: AccountTreeId::default(),
            fee_input: self.batch_fee_input,
            base_system_contracts: self.api_contracts.eth_call.clone(),
            caches: self.state.tx_sender.storage_caches().clone(),
            validation_computational_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
            chain_id: sender_config.chain_id,
            whitelisted_tokens_for_aa: self
                .state
                .tx_sender
                .read_whitelisted_tokens_for_aa_cache()
                .await,
        }
    }
}
