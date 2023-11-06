use multivm::vm_latest::constants::BLOCK_GAS_LIMIT;
use once_cell::sync::OnceCell;
use std::sync::Arc;

use multivm::interface::ExecutionResult;

use zksync_dal::ConnectionPool;
use zksync_state::PostgresStorageCaches;
use zksync_types::{
    api::{BlockId, BlockNumber, DebugCall, ResultDebugCall, TracerConfig},
    l2::L2Tx,
    transaction_request::CallRequest,
    vm_trace::Call,
    AccountTreeId, L2ChainId, H256, USED_BOOTLOADER_MEMORY_BYTES,
};
use zksync_web3_decl::error::Web3Error;

use crate::api_server::{
    execution_sandbox::{
        execute_tx_eth_call, ApiTracer, BlockArgs, TxSharedArgs, VmConcurrencyLimiter,
    },
    tx_sender::ApiContracts,
    web3::{
        backend_jsonrpc::error::internal_error,
        metrics::API_METRICS,
        resolve_block,
        state::{RpcState, SealedMiniblockNumber},
    },
};
use crate::l1_gas_price::L1GasPriceProvider;

#[derive(Debug, Clone)]
pub struct DebugNamespace {
    connection_pool: ConnectionPool,
    fair_l2_gas_price: u64,
    api_contracts: ApiContracts,
    vm_execution_cache_misses_limit: Option<usize>,
    vm_concurrency_limiter: Arc<VmConcurrencyLimiter>,
    storage_caches: PostgresStorageCaches,
    last_sealed_miniblock: SealedMiniblockNumber,
    chain_id: L2ChainId,
}

impl DebugNamespace {
    pub async fn new<G: L1GasPriceProvider>(state: RpcState<G>) -> Self {
        let sender_config = &state.tx_sender.0.sender_config;

        let api_contracts = ApiContracts::load_from_disk();
        Self {
            connection_pool: state.connection_pool,
            fair_l2_gas_price: sender_config.fair_l2_gas_price,
            api_contracts,
            vm_execution_cache_misses_limit: sender_config.vm_execution_cache_misses_limit,
            vm_concurrency_limiter: state.tx_sender.vm_concurrency_limiter(),
            storage_caches: state.tx_sender.storage_caches(),
            last_sealed_miniblock: state.last_sealed_miniblock,
            chain_id: sender_config.chain_id,
        }
    }

    #[tracing::instrument(skip(self))]
    pub async fn debug_trace_block_impl(
        &self,
        block_id: BlockId,
        options: Option<TracerConfig>,
    ) -> Result<Vec<ResultDebugCall>, Web3Error> {
        const METHOD_NAME: &str = "debug_trace_block";

        let method_latency = API_METRICS.start_block_call(METHOD_NAME, block_id);
        let only_top_call = options
            .map(|options| options.tracer_config.only_top_call)
            .unwrap_or(false);
        let mut connection = self
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap();
        let block_number = resolve_block(&mut connection, block_id, METHOD_NAME).await?;
        let call_trace = connection
            .blocks_web3_dal()
            .get_trace_for_miniblock(block_number)
            .await
            .unwrap();
        let call_trace = call_trace
            .into_iter()
            .map(|call_trace| {
                let mut result: DebugCall = call_trace.into();
                if only_top_call {
                    result.calls = vec![];
                }
                ResultDebugCall { result }
            })
            .collect();

        let block_diff = self.last_sealed_miniblock.diff(block_number);
        method_latency.observe(block_diff);
        Ok(call_trace)
    }

    #[tracing::instrument(skip(self))]
    pub async fn debug_trace_transaction_impl(
        &self,
        tx_hash: H256,
        options: Option<TracerConfig>,
    ) -> Option<DebugCall> {
        let only_top_call = options
            .map(|options| options.tracer_config.only_top_call)
            .unwrap_or(false);
        let call_trace = self
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap()
            .transactions_dal()
            .get_call_trace(tx_hash)
            .await;
        call_trace.map(|call_trace| {
            let mut result: DebugCall = call_trace.into();
            if only_top_call {
                result.calls = vec![];
            }
            result
        })
    }

    #[tracing::instrument(skip(self, request, block_id))]
    pub async fn debug_trace_call_impl(
        &self,
        request: CallRequest,
        block_id: Option<BlockId>,
        options: Option<TracerConfig>,
    ) -> Result<DebugCall, Web3Error> {
        const METHOD_NAME: &str = "debug_trace_call";

        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumber::Pending));
        let method_latency = API_METRICS.start_block_call(METHOD_NAME, block_id);
        let only_top_call = options
            .map(|options| options.tracer_config.only_top_call)
            .unwrap_or(false);

        let mut connection = self
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap();
        let block_args = BlockArgs::new(&mut connection, block_id)
            .await
            .map_err(|err| internal_error("debug_trace_call", err))?
            .ok_or(Web3Error::NoBlock)?;
        drop(connection);

        let tx = L2Tx::from_request(request.into(), USED_BOOTLOADER_MEMORY_BYTES)?;

        let shared_args = self.shared_args();
        let vm_permit = self.vm_concurrency_limiter.acquire().await;
        let vm_permit = vm_permit.ok_or(Web3Error::InternalError)?;

        // We don't need properly trace if we only need top call
        let call_tracer_result = Arc::new(OnceCell::default());
        let custom_tracers = if only_top_call {
            vec![]
        } else {
            vec![ApiTracer::CallTracer(call_tracer_result.clone())]
        };

        let result = execute_tx_eth_call(
            vm_permit,
            shared_args,
            self.connection_pool.clone(),
            tx.clone(),
            block_args,
            self.vm_execution_cache_misses_limit,
            custom_tracers,
        )
        .await;

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
            tx.common_data.fee.gas_limit.as_u32(),
            result.statistics.gas_used,
            tx.execute.value,
            tx.execute.calldata,
            output,
            revert_reason,
            trace,
        );

        let block_diff = self.last_sealed_miniblock.diff_with_block_args(&block_args);
        method_latency.observe(block_diff);
        Ok(call.into())
    }

    fn shared_args(&self) -> TxSharedArgs {
        TxSharedArgs {
            operator_account: AccountTreeId::default(),
            l1_gas_price: 100_000,
            fair_l2_gas_price: self.fair_l2_gas_price,
            base_system_contracts: self.api_contracts.eth_call.clone(),
            caches: self.storage_caches.clone(),
            validation_computational_gas_limit: BLOCK_GAS_LIMIT,
            chain_id: self.chain_id,
        }
    }
}
