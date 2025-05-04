#[cfg_attr(not(feature = "server"), allow(unused_imports))]
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use zksync_types::{api::en, tokens::TokenInfo, Address, L2BlockNumber};

use crate::{
    client::{ForWeb3Network, L2},
    types::{EcosystemContractsDto, GenesisConfigDto},
};

#[cfg_attr(
    feature = "server",
    rpc(server, client, namespace = "en", client_bounds(Self: ForWeb3Network<Net = L2>))
)]
#[cfg_attr(
    not(feature = "server"),
    rpc(client, namespace = "en", client_bounds(Self: ForWeb3Network<Net = L2>))
)]
pub trait EnNamespace {
    #[method(name = "syncL2Block")]
    async fn sync_l2_block(
        &self,
        block_number: L2BlockNumber,
        include_transactions: bool,
    ) -> RpcResult<Option<en::SyncBlock>>;

    #[method(name = "consensusGlobalConfig")]
    async fn consensus_global_config(&self) -> RpcResult<Option<en::ConsensusGlobalConfig>>;

    #[method(name = "blockMetadata")]
    async fn block_metadata(
        &self,
        block_number: L2BlockNumber,
    ) -> RpcResult<Option<en::BlockMetadata>>;

    /// Lists all tokens created at or before the specified `block_number`.
    ///
    /// This method is used by EN after snapshot recovery in order to recover token records.
    #[method(name = "syncTokens")]
    async fn sync_tokens(&self, block_number: Option<L2BlockNumber>) -> RpcResult<Vec<TokenInfo>>;

    /// Get genesis configuration
    #[method(name = "genesisConfig")]
    async fn genesis_config(&self) -> RpcResult<GenesisConfigDto>;

    /// Get tokens that are white-listed and it can be used by paymasters.
    #[method(name = "whitelistedTokensForAA")]
    async fn whitelisted_tokens_for_aa(&self) -> RpcResult<Vec<Address>>;

    #[method(name = "getEcosystemContracts")]
    async fn get_ecosystem_contracts(&self) -> RpcResult<EcosystemContractsDto>;
}
