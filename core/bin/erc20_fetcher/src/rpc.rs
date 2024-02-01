use jsonrpsee::{core::RpcResult, proc_macros::rpc};

#[rpc(server, client, namespace = "fetcher")]
pub trait RpcApi {
    /// Returns the NativeERC20/ETH conversion rate.
    #[method(name = "conversionRate")]
    async fn conversion_rate(&self) -> RpcResult<String>;

    // /// Returns the ERC20 token price in USD.
    // #[method(name = "tokenPrice")]
    // async fn price(&self) -> RpcResult<String>;

    /// returns the symbol of the ERC20.
    #[method(name = "tokenSymbol")]
    fn token_symbol(&self) -> RpcResult<String>;

    /// returns the name of the ERC20.
    #[method(name = "tokenName")]
    fn token_name(&self) -> RpcResult<String>;
}
