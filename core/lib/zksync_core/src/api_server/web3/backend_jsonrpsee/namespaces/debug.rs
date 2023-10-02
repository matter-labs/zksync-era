use zksync_types::{
    api::{BlockId, BlockNumber, DebugCall, ResultDebugCall, TracerConfig},
    transaction_request::CallRequest,
    H256,
};
use zksync_web3_decl::{
    jsonrpsee::core::{async_trait, RpcResult},
    namespaces::debug::DebugNamespaceServer,
};

use crate::api_server::web3::{backend_jsonrpsee::into_jsrpc_error, namespaces::DebugNamespace};

#[async_trait]
impl DebugNamespaceServer for DebugNamespace {
    async fn trace_block_by_number(
        &self,
        block: BlockNumber,
        options: Option<TracerConfig>,
    ) -> RpcResult<Vec<ResultDebugCall>> {
        self.debug_trace_block_impl(BlockId::Number(block), options)
            .await
            .map_err(into_jsrpc_error)
    }
    async fn trace_block_by_hash(
        &self,
        hash: H256,
        options: Option<TracerConfig>,
    ) -> RpcResult<Vec<ResultDebugCall>> {
        self.debug_trace_block_impl(BlockId::Hash(hash), options)
            .await
            .map_err(into_jsrpc_error)
    }
    async fn trace_call(
        &self,
        request: CallRequest,
        block: Option<BlockId>,
        options: Option<TracerConfig>,
    ) -> RpcResult<DebugCall> {
        self.debug_trace_call_impl(request, block, options)
            .await
            .map_err(into_jsrpc_error)
    }
    async fn trace_transaction(
        &self,
        tx_hash: H256,
        options: Option<TracerConfig>,
    ) -> RpcResult<Option<DebugCall>> {
        Ok(self.debug_trace_transaction_impl(tx_hash, options).await)
    }
}
