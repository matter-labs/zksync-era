use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use zksync_types::{api::en::SyncBlock, tokens::TokenInfo, MiniblockNumber};

#[cfg_attr(
    all(feature = "client", feature = "server"),
    rpc(server, client, namespace = "en")
)]
#[cfg_attr(
    all(feature = "client", not(feature = "server")),
    rpc(client, namespace = "en")
)]
#[cfg_attr(
    all(not(feature = "client"), feature = "server"),
    rpc(server, namespace = "en")
)]
pub trait EnNamespace {
    #[method(name = "syncL2Block")]
    async fn sync_l2_block(
        &self,
        block_number: MiniblockNumber,
        include_transactions: bool,
    ) -> RpcResult<Option<SyncBlock>>;

    /// Lists all tokens created at or before the specified `block_number`.
    ///
    /// This method is used by EN after snapshot recovery in order to recover token records.
    #[method(name = "syncTokens")]
    async fn sync_tokens(&self, block_number: Option<MiniblockNumber>)
        -> RpcResult<Vec<TokenInfo>>;
}
