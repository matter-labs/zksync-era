use zksync_types::{api::en::SyncBlock, tokens::TokenInfo, MiniblockNumber};
use zksync_web3_decl::{
    jsonrpsee::core::{async_trait, RpcResult},
    namespaces::en::EnNamespaceServer,
};

use crate::api_server::web3::{backend_jsonrpsee::into_jsrpc_error, namespaces::EnNamespace};

#[async_trait]
impl EnNamespaceServer for EnNamespace {
    async fn sync_l2_block(
        &self,
        block_number: MiniblockNumber,
        include_transactions: bool,
    ) -> RpcResult<Option<SyncBlock>> {
        self.sync_l2_block_impl(block_number, include_transactions)
            .await
            .map_err(into_jsrpc_error)
    }

    async fn sync_tokens(
        &self,
        block_number: Option<MiniblockNumber>,
    ) -> RpcResult<Vec<TokenInfo>> {
        self.sync_tokens_impl(block_number)
            .await
            .map_err(into_jsrpc_error)
    }
}
