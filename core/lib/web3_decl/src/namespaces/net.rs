#[cfg_attr(not(feature = "server"), allow(unused_imports))]
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use zksync_types::U256;

use crate::client::{ForEthereumLikeNetwork, L2};

#[cfg_attr(
    feature = "server",
    rpc(server, client, namespace = "net", client_bounds(Self: ForEthereumLikeNetwork<Net = L2>))
)]
#[cfg_attr(
    not(feature = "server"),
    rpc(client, namespace = "net", client_bounds(Self: ForEthereumLikeNetwork<Net = L2>))
)]
pub trait NetNamespace {
    #[method(name = "version")]
    fn version(&self) -> RpcResult<String>;

    #[method(name = "peerCount")]
    fn peer_count(&self) -> RpcResult<U256>;

    #[method(name = "listening")]
    fn is_listening(&self) -> RpcResult<bool>;
}
