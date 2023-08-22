use jsonrpsee::{core::RpcResult, proc_macros::rpc};

use zksync_types::{api::OpenEthActionTrace, H256};

#[cfg_attr(
    all(feature = "client", feature = "server"),
    rpc(server, client, namespace = "trace")
)]
#[cfg_attr(
    all(feature = "client", not(feature = "server")),
    rpc(client, namespace = "trace")
)]
#[cfg_attr(
    all(not(feature = "client"), feature = "server"),
    rpc(server, namespace = "trace")
)]
pub trait TraceNamespace {
    #[method(name = "trace_transaction")]
    async fn trace_transaction(&self, tx_hash: H256) -> RpcResult<Vec<OpenEthActionTrace>>;
}
