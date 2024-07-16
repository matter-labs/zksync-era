#[cfg_attr(not(feature = "server"), allow(unused_imports))]
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use zksync_types::{
    api::{BlockId, BlockNumber, DebugCall, ResultDebugCall, TracerConfig},
    debug_flat_call::DebugCallFlat,
    transaction_request::CallRequest,
};

use crate::{
    client::{ForNetwork, L2},
    types::H256,
};

#[cfg_attr(
    feature = "server",
    rpc(server, client, namespace = "debug", client_bounds(Self: ForNetwork<Net = L2>))
)]
#[cfg_attr(
    not(feature = "server"),
    rpc(client, namespace = "debug", client_bounds(Self: ForNetwork<Net = L2>))
)]
pub trait DebugNamespace {
    #[method(name = "traceBlockByNumber")]
    async fn trace_block_by_number(
        &self,
        block: BlockNumber,
        options: Option<TracerConfig>,
    ) -> RpcResult<Vec<ResultDebugCall>>;

    #[method(name = "traceBlockByNumber.callFlatTracer")]
    async fn trace_block_by_number_flat(
        &self,
        block: BlockNumber,
        options: Option<TracerConfig>,
    ) -> RpcResult<Vec<DebugCallFlat>>;

    #[method(name = "traceBlockByHash")]
    async fn trace_block_by_hash(
        &self,
        hash: H256,
        options: Option<TracerConfig>,
    ) -> RpcResult<Vec<ResultDebugCall>>;

    #[method(name = "traceCall")]
    async fn trace_call(
        &self,
        request: CallRequest,
        block: Option<BlockId>,
        options: Option<TracerConfig>,
    ) -> RpcResult<DebugCall>;

    #[method(name = "traceTransaction")]
    async fn trace_transaction(
        &self,
        tx_hash: H256,
        options: Option<TracerConfig>,
    ) -> RpcResult<Option<DebugCall>>;
}
