use std::{sync::Arc, time::Instant};

use zksync_contracts::{
    BaseSystemContracts, BaseSystemContractsHashes, PLAYGROUND_BLOCK_BOOTLOADER_CODE,
};
use zksync_dal::ConnectionPool;
use zksync_state::PostgresStorageCaches;
use zksync_types::{
    api::{BlockId, BlockNumber, DebugCall, ResultDebugCall, TracerConfig},
    l2::L2Tx,
    transaction_request::CallRequest,
    vm_trace::{Call, VmTrace},
    AccountTreeId, H256, USED_BOOTLOADER_MEMORY_BYTES,
};
use zksync_web3_decl::error::Web3Error;

use crate::api_server::{
    execution_sandbox::{execute_tx_eth_call, BlockArgs, TxSharedArgs, VmConcurrencyLimiter},
    tx_sender::SubmitTxError,
    web3::{backend_jsonrpc::error::internal_error, resolve_block},
};

#[derive(Debug, Clone)]
pub struct DebugNamespace {
    connection_pool: ConnectionPool,
    fair_l2_gas_price: u64,
    base_system_contracts: BaseSystemContracts,
    vm_execution_cache_misses_limit: Option<usize>,
    vm_concurrency_limiter: Arc<VmConcurrencyLimiter>,
    storage_caches: PostgresStorageCaches,
}

impl DebugNamespace {
    pub async fn new(
        connection_pool: ConnectionPool,
        base_system_contract_hashes: BaseSystemContractsHashes,
        fair_l2_gas_price: u64,
        vm_execution_cache_misses_limit: Option<usize>,
        vm_concurrency_limiter: Arc<VmConcurrencyLimiter>,
        storage_caches: PostgresStorageCaches,
    ) -> Self {
        let mut storage = connection_pool.access_storage_tagged("api").await;

        let mut base_system_contracts = storage
            .storage_dal()
            .get_base_system_contracts(
                base_system_contract_hashes.bootloader,
                base_system_contract_hashes.default_aa,
            )
            .await;

        drop(storage);

        base_system_contracts.bootloader = PLAYGROUND_BLOCK_BOOTLOADER_CODE.clone();
        Self {
            connection_pool,
            fair_l2_gas_price,
            base_system_contracts,
            vm_execution_cache_misses_limit,
            vm_concurrency_limiter,
            storage_caches,
        }
    }

    #[tracing::instrument(skip(self))]
    pub async fn debug_trace_block_impl(
        &self,
        block_id: BlockId,
        options: Option<TracerConfig>,
    ) -> Result<Vec<ResultDebugCall>, Web3Error> {
        const METHOD_NAME: &str = "debug_trace_block";

        let start = Instant::now();
        let only_top_call = options
            .map(|options| options.tracer_config.only_top_call)
            .unwrap_or(false);
        let mut connection = self.connection_pool.access_storage_tagged("api").await;
        let block_number = resolve_block(&mut connection, block_id, METHOD_NAME).await?;
        let call_trace = connection
            .blocks_web3_dal()
            .get_trace_for_miniblock(block_number)
            .await;

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => METHOD_NAME, "block_id" => block_id.extract_block_tag());

        Ok(call_trace
            .into_iter()
            .map(|call_trace| {
                let mut result: DebugCall = call_trace.into();
                if only_top_call {
                    result.calls = vec![];
                }
                ResultDebugCall { result }
            })
            .collect())
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
        let start = Instant::now();
        let only_top_call = options
            .map(|options| options.tracer_config.only_top_call)
            .unwrap_or(false);

        let block = block_id.unwrap_or(BlockId::Number(BlockNumber::Pending));
        let mut connection = self.connection_pool.access_storage_tagged("api").await;
        let block_args = BlockArgs::new(&mut connection, block)
            .await
            .map_err(|err| internal_error("debug_trace_call", err))?
            .ok_or(Web3Error::NoBlock)?;
        drop(connection);

        let tx = L2Tx::from_request(request.into(), USED_BOOTLOADER_MEMORY_BYTES)?;

        let shared_args = self.shared_args();
        let vm_permit = self.vm_concurrency_limiter.acquire().await;
        let vm_permit = vm_permit.ok_or(Web3Error::InternalError)?;

        // We don't need properly trace if we only need top call
        let result = execute_tx_eth_call(
            vm_permit,
            shared_args,
            self.connection_pool.clone(),
            tx.clone(),
            block_args,
            self.vm_execution_cache_misses_limit,
            !only_top_call,
        )
        .await
        .map_err(|err| {
            let submit_tx_error = SubmitTxError::from(err);
            Web3Error::SubmitTransactionError(submit_tx_error.to_string(), submit_tx_error.data())
        })?;

        let (output, revert_reason) = match result.revert_reason {
            Some(result) => (vec![], Some(result.revert_reason.to_string())),
            None => (
                result
                    .return_data
                    .into_iter()
                    .flat_map(<[u8; 32]>::from)
                    .collect(),
                None,
            ),
        };
        let trace = match result.trace {
            VmTrace::CallTrace(trace) => trace,
            VmTrace::ExecutionTrace(_) => vec![],
        };
        let call = Call::new_high_level(
            u32::MAX,
            result.gas_used,
            tx.execute.value,
            tx.execute.calldata,
            output,
            revert_reason,
            trace,
        );

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => METHOD_NAME, "block_id" => block.extract_block_tag());

        Ok(call.into())
    }

    fn shared_args(&self) -> TxSharedArgs {
        TxSharedArgs {
            operator_account: AccountTreeId::default(),
            l1_gas_price: 100_000,
            fair_l2_gas_price: self.fair_l2_gas_price,
            base_system_contracts: self.base_system_contracts.clone(),
            caches: self.storage_caches.clone(),
        }
    }
}
