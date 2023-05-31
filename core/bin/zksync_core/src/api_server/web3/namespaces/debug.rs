use crate::api_server::execution_sandbox::execute_tx_eth_call;
use crate::api_server::web3::backend_jsonrpc::namespaces::debug::TracerConfig;
use std::time::Instant;
use zksync_contracts::{
    BaseSystemContracts, BaseSystemContractsHashes, PLAYGROUND_BLOCK_BOOTLOADER_CODE,
};
use zksync_dal::ConnectionPool;
use zksync_types::api::{BlockId, BlockNumber, DebugCall, ResultDebugCall};
use zksync_types::transaction_request::{l2_tx_from_call_req, CallRequest};
use zksync_types::vm_trace::{Call, VmTrace};
use zksync_types::{H256, USED_BOOTLOADER_MEMORY_BYTES};
use zksync_web3_decl::error::Web3Error;

#[derive(Debug, Clone)]
pub struct DebugNamespace {
    pub connection_pool: ConnectionPool,
    pub fair_l2_gas_price: u64,
    pub base_system_contracts: BaseSystemContracts,
    pub vm_execution_cache_misses_limit: Option<usize>,
}

impl DebugNamespace {
    pub fn new(
        connection_pool: ConnectionPool,
        base_system_contract_hashes: BaseSystemContractsHashes,
        fair_l2_gas_price: u64,
        vm_execution_cache_misses_limit: Option<usize>,
    ) -> Self {
        let mut storage = connection_pool.access_storage_blocking();

        let mut base_system_contracts = storage.storage_dal().get_base_system_contracts(
            base_system_contract_hashes.bootloader,
            base_system_contract_hashes.default_aa,
        );

        drop(storage);

        base_system_contracts.bootloader = PLAYGROUND_BLOCK_BOOTLOADER_CODE.clone();
        Self {
            connection_pool,
            fair_l2_gas_price,
            base_system_contracts,
            vm_execution_cache_misses_limit,
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn debug_trace_block_impl(
        &self,
        block: BlockId,
        options: Option<TracerConfig>,
    ) -> Result<Vec<ResultDebugCall>, Web3Error> {
        let only_top_call = options
            .map(|options| options.tracer_config.only_top_call)
            .unwrap_or(false);
        let call_trace = self
            .connection_pool
            .access_storage_blocking()
            .blocks_web3_dal()
            .get_trace_for_miniblock(block)?;
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
    pub fn debug_trace_transaction_impl(
        &self,
        tx_hash: H256,
        options: Option<TracerConfig>,
    ) -> Option<DebugCall> {
        let only_top_call = options
            .map(|options| options.tracer_config.only_top_call)
            .unwrap_or(false);
        let call_trace = self
            .connection_pool
            .access_storage_blocking()
            .transactions_dal()
            .get_call_trace(tx_hash);
        call_trace.map(|call_trace| {
            let mut result: DebugCall = call_trace.into();
            if only_top_call {
                result.calls = vec![];
            }
            result
        })
    }

    #[tracing::instrument(skip(self, request, block))]
    pub fn debug_trace_call_impl(
        &self,
        request: CallRequest,
        block: Option<BlockId>,
        options: Option<TracerConfig>,
    ) -> Result<DebugCall, Web3Error> {
        let start = Instant::now();
        let only_top_call = options
            .map(|options| options.tracer_config.only_top_call)
            .unwrap_or(false);
        let block = block.unwrap_or(BlockId::Number(BlockNumber::Pending));
        let tx = l2_tx_from_call_req(request, USED_BOOTLOADER_MEMORY_BYTES)?;

        let enforced_base_fee = Some(tx.common_data.fee.max_fee_per_gas.as_u64());
        // We don't need properly trace if we only need top call
        let result = execute_tx_eth_call(
            &self.connection_pool,
            tx.clone(),
            block,
            100000,
            self.fair_l2_gas_price,
            enforced_base_fee,
            &self.base_system_contracts,
            self.vm_execution_cache_misses_limit,
            !only_top_call,
        )?;

        let (output, revert_reason) = match result.revert_reason {
            Some(result) => (vec![], Some(result.revert_reason.to_string())),
            None => (
                result
                    .return_data
                    .into_iter()
                    .flat_map(|val| {
                        let bytes: [u8; 32] = val.into();
                        bytes.to_vec()
                    })
                    .collect::<Vec<_>>(),
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

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => "debug_trace_call");
        Ok(call.into())
    }
}
