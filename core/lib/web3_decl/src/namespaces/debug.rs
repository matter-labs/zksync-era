#[cfg_attr(not(feature = "server"), allow(unused_imports))]
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use zksync_types::{
    api::{BlockId, BlockNumber, CallTracerBlockResult, CallTracerResult, TracerConfig},
    transaction_request::CallRequest,
    web3::Bytes,
};

use crate::{
    client::{ForWeb3Network, L2},
    types::H256,
};

#[cfg_attr(
    feature = "server",
    rpc(server, client, namespace = "debug", client_bounds(Self: ForWeb3Network<Net = L2>))
)]
#[cfg_attr(
    not(feature = "server"),
    rpc(client, namespace = "debug", client_bounds(Self: ForWeb3Network<Net = L2>))
)]
pub trait DebugNamespace {
    #[method(name = "traceBlockByNumber")]
    async fn trace_block_by_number(
        &self,
        block: BlockNumber,
        options: Option<TracerConfig>,
    ) -> RpcResult<CallTracerBlockResult>;

    #[method(name = "traceBlockByHash")]
    async fn trace_block_by_hash(
        &self,
        hash: H256,
        options: Option<TracerConfig>,
    ) -> RpcResult<CallTracerBlockResult>;

    #[method(name = "traceCall")]
    async fn trace_call(
        &self,
        request: CallRequest,
        block: Option<BlockId>,
        options: Option<TracerConfig>,
    ) -> RpcResult<CallTracerResult>;

    #[method(name = "traceTransaction")]
    async fn trace_transaction(
        &self,
        tx_hash: H256,
        options: Option<TracerConfig>,
    ) -> RpcResult<Option<CallTracerResult>>;

    #[method(name = "getRawTransaction")]
    async fn get_raw_transaction(&self, tx_hash: H256) -> RpcResult<Option<Bytes>>;

    #[method(name = "getRawTransactions")]
    async fn get_raw_transactions(&self, block: BlockId) -> RpcResult<Vec<Bytes>>;
}
