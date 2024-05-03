use jsonrpsee::{core::RpcResult, proc_macros::rpc};

#[cfg_attr(
    all(feature = "client", feature = "server"),
    rpc(server, client, namespace = "web3")
)]
#[cfg_attr(
    all(feature = "client", not(feature = "server")),
    rpc(client, namespace = "web3")
)]
#[cfg_attr(
    all(not(feature = "client"), feature = "server"),
    rpc(server, namespace = "web3")
)]
pub trait Web3Namespace {
    #[method(name = "clientVersion")]
    fn client_version(&self) -> RpcResult<String>;

    // `sha3` method is intentionally not implemented for the main server implementation:
    // it can easily be implemented on the user side.
}
