use zksync_types::{api::en, tokens::TokenInfo, Address, L2BlockNumber};
use zksync_web3_decl::{
    jsonrpsee::core::{async_trait, RpcResult},
    namespaces::EnNamespaceServer,
    types::{EcosystemContractsDto, GenesisConfigDto},
};

use crate::web3::namespaces::EnNamespace;

#[async_trait]
impl EnNamespaceServer for EnNamespace {
    async fn sync_l2_block(
        &self,
        block_number: L2BlockNumber,
        include_transactions: bool,
    ) -> RpcResult<Option<en::SyncBlock>> {
        self.sync_l2_block_impl(block_number, include_transactions)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn consensus_global_config(&self) -> RpcResult<Option<en::ConsensusGlobalConfig>> {
        self.consensus_global_config_impl()
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn block_metadata(
        &self,
        block_number: L2BlockNumber,
    ) -> RpcResult<Option<en::BlockMetadata>> {
        self.block_metadata_impl(block_number)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn sync_tokens(&self, block_number: Option<L2BlockNumber>) -> RpcResult<Vec<TokenInfo>> {
        self.sync_tokens_impl(block_number)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn genesis_config(&self) -> RpcResult<GenesisConfigDto> {
        self.genesis_config_impl()
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn whitelisted_tokens_for_aa(&self) -> RpcResult<Vec<Address>> {
        self.whitelisted_tokens_for_aa_impl()
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_ecosystem_contracts(&self) -> RpcResult<EcosystemContractsDto> {
        self.get_l1_ecosystem_contracts_impl()
            .await
            .map_err(|err| self.current_method().map_err(err))
    }
}
