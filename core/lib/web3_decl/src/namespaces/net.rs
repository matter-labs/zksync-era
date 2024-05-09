#[cfg_attr(not(feature = "server"), allow(unused_imports))]
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use zksync_types::U256;

#[cfg(feature = "client")]
use crate::client::{ForNetwork, L2};

#[cfg_attr(
    all(feature = "client", feature = "server"),
    rpc(server, client, namespace = "net", client_bounds(Self: ForNetwork<Net = L2>))
)]
#[cfg_attr(
    all(feature = "client", not(feature = "server")),
    rpc(client, namespace = "net", client_bounds(Self: ForNetwork<Net = L2>))
)]
#[cfg_attr(
    all(not(feature = "client"), feature = "server"),
    rpc(server, namespace = "net")
)]
pub trait NetNamespace {
    #[method(name = "version")]
    fn version(&self) -> RpcResult<String>;

    #[method(name = "peerCount")]
    fn peer_count(&self) -> RpcResult<U256>;

    #[method(name = "listening")]
    fn is_listening(&self) -> RpcResult<bool>;
}
