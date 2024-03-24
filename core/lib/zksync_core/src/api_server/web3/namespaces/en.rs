use anyhow::Context as _;
use zksync_config::GenesisConfig;
use zksync_dal::CoreDal;
use zksync_types::{api::en, tokens::TokenInfo, L1BatchNumber, MiniblockNumber, H256};
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

    #[tracing::instrument(skip(self))]
    pub async fn genesis_config_impl(&self) -> Result<GenesisConfig, Web3Error> {
        // If this method will cause some load, we can cache everything in memory
        let mut storage = self.state.connection_pool.connection_tagged("api").await?;
        let genesis_batch = storage
            .blocks_dal()
            .get_storage_l1_batch(L1BatchNumber(0))
            .await
            .context("genesis_config")?
            .context("Genesis batch doesn't exist")?;

        let protocol_version = genesis_batch
            .protocol_version
            .context("Genesis is not finished")? as u16;
        let verifier_config = storage
            .protocol_versions_dal()
            .l1_verifier_config_for_version(protocol_version.try_into().unwrap())
            .await
            .context("Genesis is not finished")?;
        let fee_account = storage
            .blocks_dal()
            .get_fee_address_for_miniblock(MiniblockNumber(0))
            .await
            .context("genesis_config")?
            .context("Genesis not finished")?;
        let config = GenesisConfig {
            protocol_version,
            genesis_root_hash: H256::from_slice(
                &genesis_batch.hash.context("Genesis is not finished")?,
            ),
            rollup_last_leaf_index: genesis_batch
                .rollup_last_leaf_index
                .context("Genesis is not finished")? as u64,
            genesis_commitment: H256::from_slice(
                &genesis_batch
                    .commitment
                    .context("Genesis is not finished")?,
            ),
            bootloader_hash: H256::from_slice(
                &genesis_batch
                    .bootloader_code_hash
                    .context("Genesis is not finished")?,
            ),
            default_aa_hash: H256::from_slice(
                &genesis_batch
                    .default_aa_code_hash
                    .context("Genesis is not finished")?,
            ),
            fee_account,
            l1_chain_id: self.state.api_config.l1_chain_id,

            l2_chain_id: self.state.api_config.l2_chain_id,
            recursion_node_level_vk_hash: verifier_config.params.recursion_node_level_vk_hash,
            recursion_leaf_level_vk_hash: verifier_config.params.recursion_leaf_level_vk_hash,
            recursion_scheduler_level_vk_hash: verifier_config.recursion_scheduler_level_vk_hash,
        };
        dbg!(&config);
        Ok(config)
    }
}
