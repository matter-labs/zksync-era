use anyhow::Context as _;
use zksync_config::{configs::EcosystemContracts, GenesisConfig};
use zksync_dal::{CoreDal, DalError};
use zksync_types::{
    api::en, protocol_version::ProtocolSemanticVersion, tokens::TokenInfo, Address, L1BatchNumber,
    L2BlockNumber,
};
use zksync_web3_decl::error::Web3Error;

use crate::web3::{backend_jsonrpsee::MethodTracer, state::RpcState};

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
        let mut storage = self.state.acquire_connection().await?;
        let Some(genesis) = storage
            .consensus_dal()
            .genesis()
            .await
            .map_err(DalError::generalize)?
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

    pub async fn sync_l2_block_impl(
        &self,
        block_number: L2BlockNumber,
        include_transactions: bool,
    ) -> Result<Option<en::SyncBlock>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        Ok(storage
            .sync_dal()
            .sync_block(block_number, include_transactions)
            .await
            .map_err(DalError::generalize)?)
    }

    pub async fn sync_tokens_impl(
        &self,
        block_number: Option<L2BlockNumber>,
    ) -> Result<Vec<TokenInfo>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        Ok(storage
            .tokens_web3_dal()
            .get_all_tokens(block_number)
            .await
            .map_err(DalError::generalize)?)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_ecosystem_contracts_impl(&self) -> Result<EcosystemContracts, Web3Error> {
        Ok(self
            .state
            .api_config
            .bridgehub_proxy_addr
            .map(|bridgehub_proxy_addr| EcosystemContracts {
                bridgehub_proxy_addr,
                state_transition_proxy_addr: self
                    .state
                    .api_config
                    .state_transition_proxy_addr
                    .unwrap(),
                transparent_proxy_admin_addr: self
                    .state
                    .api_config
                    .transparent_proxy_admin_addr
                    .unwrap(),
            })
            .context("Shared bridge doesn't supported")?)
    }

    #[tracing::instrument(skip(self))]
    pub async fn genesis_config_impl(&self) -> Result<GenesisConfig, Web3Error> {
        // If this method will cause some load, we can cache everything in memory
        let mut storage = self.state.acquire_connection().await?;
        let genesis_batch = storage
            .blocks_dal()
            .get_l1_batch_metadata(L1BatchNumber(0))
            .await
            .map_err(DalError::generalize)?
            .context("Genesis batch doesn't exist")?;
        let minor = genesis_batch
            .header
            .protocol_version
            .context("Genesis is not finished")?;
        let patch = storage
            .protocol_versions_dal()
            .first_patch_for_version(minor)
            .await
            .map_err(DalError::generalize)?
            .context("Genesis is not finished")?;
        let protocol_version = ProtocolSemanticVersion { minor, patch };
        let verifier_config = storage
            .protocol_versions_dal()
            .l1_verifier_config_for_version(protocol_version)
            .await
            .context("Genesis is not finished")?;
        let fee_account = storage
            .blocks_dal()
            .get_fee_address_for_l2_block(L2BlockNumber(0))
            .await
            .map_err(DalError::generalize)?
            .context("Genesis not finished")?;

        let config = GenesisConfig {
            protocol_version: Some(protocol_version),
            genesis_root_hash: Some(genesis_batch.metadata.root_hash),
            rollup_last_leaf_index: Some(genesis_batch.metadata.rollup_last_leaf_index),
            genesis_commitment: Some(genesis_batch.metadata.commitment),
            bootloader_hash: Some(genesis_batch.header.base_system_contracts_hashes.bootloader),
            default_aa_hash: Some(genesis_batch.header.base_system_contracts_hashes.default_aa),
            l1_chain_id: self.state.api_config.l1_chain_id,
            l2_chain_id: self.state.api_config.l2_chain_id,
            recursion_node_level_vk_hash: verifier_config.params.recursion_node_level_vk_hash,
            recursion_leaf_level_vk_hash: verifier_config.params.recursion_leaf_level_vk_hash,
            recursion_circuits_set_vks_hash: Default::default(),
            recursion_scheduler_level_vk_hash: verifier_config.recursion_scheduler_level_vk_hash,
            fee_account,
            dummy_verifier: self.state.api_config.dummy_verifier,
            l1_batch_commit_data_generator_mode: self
                .state
                .api_config
                .l1_batch_commit_data_generator_mode,
        };
        Ok(config)
    }

    pub async fn whitelisted_tokens_for_aa_impl(&self) -> Result<Vec<Address>, Web3Error> {
        Ok(self
            .state
            .tx_sender
            .read_whitelisted_tokens_for_aa_cache()
            .await)
    }
}
