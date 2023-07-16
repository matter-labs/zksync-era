use crate::types::H256;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};

use zksync_types::api::{BlockId, BlockNumber, DebugCall, ResultDebugCall, TracerConfig};
use zksync_types::transaction_request::CallRequest;

#[cfg_attr(
    all(feature = "client", feature = "server"),
    rpc(server, client, namespace = "debug")
)]
#[cfg_attr(
    all(feature = "client", not(feature = "server")),
    rpc(client, namespace = "debug")
)]
#[cfg_attr(
    all(not(feature = "client"), feature = "server"),
    rpc(server, namespace = "debug")
)]
pub trait DebugNamespace {
    #[method(name = "traceBlockByNumber")]
    async fn trace_block_by_number(
        &self,
        block: BlockNumber,
        options: Option<TracerConfig>,
    ) -> RpcResult<Vec<ResultDebugCall>>;
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
