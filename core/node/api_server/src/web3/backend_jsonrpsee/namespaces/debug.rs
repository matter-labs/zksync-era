use zksync_types::{
    api::{BlockId, BlockNumber, DebugCall, ResultDebugCall, TracerConfig},
    debug_flat_call::DebugCallFlat,
    transaction_request::CallRequest,
    H256,
};
use zksync_web3_decl::{
    jsonrpsee::core::{async_trait, RpcResult},
    namespaces::DebugNamespaceServer,
};

use crate::web3::namespaces::DebugNamespace;

#[async_trait]
impl DebugNamespaceServer for DebugNamespace {
    async fn trace_block_by_number(
        &self,
        block: BlockNumber,
        options: Option<TracerConfig>,
    ) -> RpcResult<Vec<ResultDebugCall>> {
        self.debug_trace_block_impl(BlockId::Number(block), options)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn trace_block_by_number_flat(
        &self,
        block: BlockNumber,
        options: Option<TracerConfig>,
    ) -> RpcResult<Vec<DebugCallFlat>> {
        self.debug_trace_block_flat_impl(BlockId::Number(block), options)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn trace_block_by_hash(
        &self,
        hash: H256,
        options: Option<TracerConfig>,
    ) -> RpcResult<Vec<ResultDebugCall>> {
        self.debug_trace_block_impl(BlockId::Hash(hash), options)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn trace_call(
        &self,
        request: CallRequest,
        block: Option<BlockId>,
        options: Option<TracerConfig>,
    ) -> RpcResult<DebugCall> {
        self.debug_trace_call_impl(request, block, options)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn trace_transaction(
        &self,
        tx_hash: H256,
        options: Option<TracerConfig>,
    ) -> RpcResult<Option<DebugCall>> {
        self.debug_trace_transaction_impl(tx_hash, options)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }
}
