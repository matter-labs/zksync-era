use anyhow::Context as _;
use zksync_dal::CoreDal;
use zksync_types::{api::en, tokens::TokenInfo, MiniblockNumber};
use zksync_web3_decl::error::Web3Error;

use crate::api_server::web3::{backend_jsonrpsee::MethodTracer, state::RpcState};

/// Namespace for External Node unique methods.
/// Main use case for it is the EN synchronization.
#[derive(Debug)]
pub(crate) struct EnNamespace {
    state: RpcState,
}

impl EnNamespace {
    pub fn new(state: RpcState) -> Self {
        Self { state }
    }

    pub async fn consensus_genesis_impl(&self) -> Result<Option<en::ConsensusGenesis>, Web3Error> {
        let Some(genesis) = self
            .state
            .connection_pool
            .connection_tagged("api")
            .await?
            .consensus_dal()
            .genesis()
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(en::ConsensusGenesis(
            zksync_protobuf::serde::serialize(&genesis, serde_json::value::Serializer).unwrap(),
        )))
    }

    pub(crate) fn current_method(&self) -> &MethodTracer {
        &self.state.current_method
    }

    #[tracing::instrument(skip(self))]
    pub async fn sync_l2_block_impl(
        &self,
        block_number: MiniblockNumber,
        include_transactions: bool,
    ) -> Result<Option<en::SyncBlock>, Web3Error> {
        let mut storage = self.state.connection_pool.connection_tagged("api").await?;
        Ok(storage
            .sync_dal()
            .sync_block(block_number, include_transactions)
            .await
            .context("sync_block")?)
    }

    #[tracing::instrument(skip(self))]
    pub async fn sync_tokens_impl(
        &self,
        block_number: Option<MiniblockNumber>,
    ) -> Result<Vec<TokenInfo>, Web3Error> {
        let mut storage = self.state.connection_pool.connection_tagged("api").await?;
        Ok(storage
            .tokens_web3_dal()
            .get_all_tokens(block_number)
            .await
            .context("get_all_tokens")?)
    }
}
