#[cfg_attr(not(feature = "server"), allow(unused_imports))]
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;

use crate::client::{ForWeb3Network, L2};

#[cfg_attr(
    feature = "server",
    rpc(server, client, namespace = "web3", client_bounds(Self: ForWeb3Network<Net = L2>))
)]
#[cfg_attr(
    not(feature = "server"),
    rpc(client, namespace = "web3", client_bounds(Self: ForWeb3Network<Net = L2>))
)]
pub trait Web3Namespace {
    #[method(name = "clientVersion")]
    fn client_version(&self) -> RpcResult<String>;

    // `sha3` method is intentionally not implemented for the main server implementation:
    // it can easily be implemented on the user side.
}
