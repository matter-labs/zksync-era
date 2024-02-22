use zksync_types::{api::en, tokens::TokenInfo, MiniblockNumber};
use zksync_web3_decl::error::Web3Error;

use crate::api_server::web3::{backend_jsonrpsee::internal_error, state::RpcState};

/// Namespace for External Node unique methods.
/// Main use case for it is the EN synchronization.
#[derive(Debug)]
pub struct EnNamespace {
    state: RpcState,
}

impl EnNamespace {
    pub fn new(state: RpcState) -> Self {
        Self { state }
    }

    pub async fn consensus_genesis_impl(&self) -> Result<Option<en::ConsensusGenesis>, Web3Error> {
        async {
            let Some(genesis) = self
                .state
                .connection_pool
                .access_storage_tagged("api")
                .await?
                .consensus_dal()
                .genesis()
                .await?
            else {
                return Ok(None);
            };
            anyhow::Ok(Some(en::ConsensusGenesis(
                zksync_protobuf::serde::serialize(&genesis, serde_json::value::Serializer).unwrap(),
            )))
        }
        .await
        .map_err(|err| internal_error("en_consensusGenesis", err))
    }

    #[tracing::instrument(skip(self))]
    pub async fn sync_l2_block_impl(
        &self,
        block_number: MiniblockNumber,
        include_transactions: bool,
    ) -> Result<Option<en::SyncBlock>, Web3Error> {
        const METHOD_NAME: &str = "en_syncL2Block";

        let mut storage = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?;
        storage
            .sync_dal()
            .sync_block(block_number, include_transactions)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))
    }

    #[tracing::instrument(skip(self))]
    pub async fn sync_tokens_impl(
        &self,
        block_number: Option<MiniblockNumber>,
    ) -> Result<Vec<TokenInfo>, Web3Error> {
        const METHOD_NAME: &str = "sync_tokens";

        let mut storage = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?;
        storage
            .tokens_web3_dal()
            .get_all_tokens(block_number)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))
    }
}
