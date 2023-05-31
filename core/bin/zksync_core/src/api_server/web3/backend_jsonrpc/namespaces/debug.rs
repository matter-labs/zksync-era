// External uses
use crate::api_server::web3::backend_jsonrpc::error::into_jsrpc_error;
use crate::api_server::web3::namespaces::debug::DebugNamespace;
use jsonrpc_core::Result;
use jsonrpc_derive::rpc;
use serde::{Deserialize, Serialize};
use zksync_types::api::{BlockId, BlockNumber, DebugCall, ResultDebugCall};
use zksync_types::transaction_request::CallRequest;
use zksync_types::H256;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SupportedTracers {
    CallTracer,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallTracerConfig {
    pub only_top_call: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TracerConfig {
    pub tracer: SupportedTracers,
    pub tracer_config: CallTracerConfig,
}

#[rpc]
pub trait DebugNamespaceT {
    #[rpc(name = "debug_traceBlockByNumber", returns = "Vec<ResultDebugCall>")]
    fn trace_block_by_number(
        &self,
        block: BlockNumber,
        options: Option<TracerConfig>,
    ) -> Result<Vec<ResultDebugCall>>;
    #[rpc(name = "debug_traceBlockByHash", returns = "Vec<ResultDebugCall>")]
    fn trace_block_by_hash(
        &self,
        hash: H256,
        options: Option<TracerConfig>,
    ) -> Result<Vec<ResultDebugCall>>;
    #[rpc(name = "debug_traceCall", returns = "DebugCall")]
    fn trace_call(
        &self,
        request: CallRequest,
        block: Option<BlockId>,
        options: Option<TracerConfig>,
    ) -> Result<DebugCall>;
    #[rpc(name = "debug_traceTransaction", returns = "DebugCall")]
    fn trace_transaction(
        &self,
        tx_hash: H256,
        options: Option<TracerConfig>,
    ) -> Result<Option<DebugCall>>;
}

impl DebugNamespaceT for DebugNamespace {
    fn trace_block_by_number(
        &self,
        block: BlockNumber,
        options: Option<TracerConfig>,
    ) -> Result<Vec<ResultDebugCall>> {
        self.debug_trace_block_impl(BlockId::Number(block), options)
            .map_err(into_jsrpc_error)
    }

    fn trace_block_by_hash(
        &self,
        hash: H256,
        options: Option<TracerConfig>,
    ) -> Result<Vec<ResultDebugCall>> {
        self.debug_trace_block_impl(BlockId::Hash(hash), options)
            .map_err(into_jsrpc_error)
    }

    fn trace_call(
        &self,
        request: CallRequest,
        block: Option<BlockId>,
        options: Option<TracerConfig>,
    ) -> Result<DebugCall> {
        self.debug_trace_call_impl(request, block, options)
            .map_err(into_jsrpc_error)
    }

    fn trace_transaction(
        &self,
        tx_hash: H256,
        options: Option<TracerConfig>,
    ) -> Result<Option<DebugCall>> {
        Ok(self.debug_trace_transaction_impl(tx_hash, options))
    }
}
