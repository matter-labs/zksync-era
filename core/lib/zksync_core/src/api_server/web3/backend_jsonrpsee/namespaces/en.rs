use zksync_config::GenesisConfig;
use zksync_types::{api::en, tokens::TokenInfo, MiniblockNumber};
use zksync_web3_decl::{
    jsonrpsee::core::{async_trait, RpcResult},
    namespaces::en::EnNamespaceServer,
};

use crate::api_server::web3::namespaces::EnNamespace;

#[async_trait]
impl EnNamespaceServer for EnNamespace {
    async fn sync_l2_block(
        &self,
        block_number: MiniblockNumber,
        include_transactions: bool,
    ) -> RpcResult<Option<en::SyncBlock>> {
        self.sync_l2_block_impl(block_number, include_transactions)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn consensus_genesis(&self) -> RpcResult<Option<en::ConsensusGenesis>> {
        self.consensus_genesis_impl()
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn sync_tokens(
        &self,
        block_number: Option<MiniblockNumber>,
    ) -> RpcResult<Vec<TokenInfo>> {
        self.sync_tokens_impl(block_number)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn genesis_config(&self) -> RpcResult<GenesisConfig> {
        self.genesis_config_impl()
            .await
            .map_err(|err| self.current_method().map_err(err))
    }
}
