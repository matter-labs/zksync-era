use zksync_types::{
    api::{BlockId, BlockNumber, CallTracerBlockResult, CallTracerResult, TracerConfig},
    transaction_request::CallRequest,
    web3::Bytes,
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
    ) -> RpcResult<CallTracerBlockResult> {
        self.debug_trace_block_impl(BlockId::Number(block), options)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn trace_block_by_hash(
        &self,
        hash: H256,
        options: Option<TracerConfig>,
    ) -> RpcResult<CallTracerBlockResult> {
        self.debug_trace_block_impl(BlockId::Hash(hash), options)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn trace_call(
        &self,
        request: CallRequest,
        block: Option<BlockId>,
        options: Option<TracerConfig>,
    ) -> RpcResult<CallTracerResult> {
        self.debug_trace_call_impl(request, block, options)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn trace_transaction(
        &self,
        tx_hash: H256,
        options: Option<TracerConfig>,
    ) -> RpcResult<Option<CallTracerResult>> {
        self.debug_trace_transaction_impl(tx_hash, options)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_raw_transaction(&self, tx_hash: H256) -> RpcResult<Option<Bytes>> {
        self.debug_get_raw_transaction_impl(tx_hash)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_raw_transactions(&self, block: BlockId) -> RpcResult<Vec<Bytes>> {
        self.debug_get_raw_transactions_impl(block)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }
}
