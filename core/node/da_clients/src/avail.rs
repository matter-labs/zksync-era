use std::{
    fmt::{Debug, Formatter},
    sync::Arc,
};

use alloy::{
    primitives::{B256, U256},
    sol,
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use avail_core::AppId;
use avail_subxt::{
    api::{
        self, data_availability::calls::types::SubmitData,
        runtime_types::bounded_collections::bounded_vec::BoundedVec,
    },
    tx, AvailClient as AvailSubxtClient,
};
use serde::Deserialize;
use subxt_signer::{bip39::Mnemonic, sr25519::Keypair};
use zksync_config::AvailConfig;
use zksync_da_client::{
    types::{DAError, DispatchResponse, InclusionData},
    DataAvailabilityClient,
};

/// An implementation of the `DataAvailabilityClient` trait that interacts with the Avail network.
#[derive(Clone)]
pub struct AvailClient {
    config: AvailConfig,
    client: Arc<AvailSubxtClient>,
    #[allow(unused)]
    api_client: Arc<reqwest::Client>,
    keypair: Keypair,
}

#[derive(Deserialize)]
#[allow(unused)]
pub struct BridgeAPIResponse {
    blob_root: B256,
    bridge_root: B256,
    data_root_index: U256,
    data_root_proof: Vec<B256>,
    leaf: B256,
    leaf_index: U256,
    leaf_proof: Vec<B256>,
    range_hash: B256,
}

// Not used in the V1 implementation
sol! {
    struct MerkleProofInput {
        // proof of inclusion for the data root
        bytes32[] dataRootProof;
        // proof of inclusion of leaf within blob/bridge root
        bytes32[] leafProof;
        // abi.encodePacked(startBlock, endBlock) of header range commitment on vectorx
        bytes32 rangeHash;
        // index of the data root in the commitment tree
        uint256 dataRootIndex;
        // blob root to check proof against, or reconstruct the data root
        bytes32 blobRoot;
        // bridge root to check proof against, or reconstruct the data root
        bytes32 bridgeRoot;
        // leaf being proven
        bytes32 leaf;
        // index of the leaf in the blob/bridge root tree
        uint256 leafIndex;
    }
}

impl AvailClient {
    const MAX_BLOB_SIZE: usize = 512 * 1024; // 512kb

    pub async fn new(config: AvailConfig) -> Result<Self> {
        let client = AvailSubxtClient::new(config.api_node_url.clone())
            .await
            .map_err(to_non_retriable_da_error)?;

        let mnemonic = Mnemonic::parse(&config.seed).map_err(to_non_retriable_da_error)?;

        let keypair = Keypair::from_phrase(&mnemonic, None).map_err(to_non_retriable_da_error)?;

        let api_client = reqwest::Client::new();

        Ok(Self {
            config,
            client: client.into(),
            api_client: api_client.into(),
            keypair,
        })
    }
}

pub fn to_non_retriable_da_error(error: impl Into<anyhow::Error>) -> DAError {
    DAError {
        error: error.into(),
        is_retriable: false,
    }
}

#[async_trait]
impl DataAvailabilityClient for AvailClient {
    async fn dispatch_blob(
        &self,
        _batch_number: u32,
        data: Vec<u8>,
    ) -> Result<DispatchResponse, DAError> {
        let call = api::tx()
            .data_availability()
            .submit_data(BoundedVec(data.clone()));
        let tx_progress = tx::send(
            &self.client,
            &call,
            &self.keypair,
            AppId(self.config.app_id),
        )
        .await
        .map_err(to_non_retriable_da_error)?;
        let block_hash = tx::then_in_block(tx_progress)
            .await
            .map_err(to_non_retriable_da_error)?
            .block_hash();

        // Retrieve the data from the block hash
        let block = self
            .client
            .blocks()
            .at(block_hash)
            .await
            .map_err(to_non_retriable_da_error)?;
        let extrinsics = block
            .extrinsics()
            .await
            .map_err(to_non_retriable_da_error)?;
        let mut found = false;
        let mut tx_idx = 0;

        // Can't be done without iterating over all extrinsics
        for ext in extrinsics.iter() {
            let ext = ext.map_err(to_non_retriable_da_error)?;
            let call = ext.as_extrinsic::<SubmitData>();
            if let Ok(Some(call)) = call {
                if data.clone() == call.data.0 {
                    found = true;
                    break;
                }
            }
            tx_idx += 1;
        }

        if !found {
            return Err(to_non_retriable_da_error(anyhow!(
                "No DA submission found in block: {}",
                block_hash
            )));
        }

        Ok(DispatchResponse {
            blob_id: format!("{}:{}", block_hash, tx_idx),
        })
    }

    async fn get_inclusion_data(&self, _blob_id: &str) -> Result<Option<InclusionData>, DAError> {
        // TODO: implement inclusion data retrieval
        Ok(Some(InclusionData { data: vec![] }))
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> Option<usize> {
        Some(AvailClient::MAX_BLOB_SIZE)
    }
}

impl Debug for AvailClient {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AvailClient")
            .field("api_node_url", &self.config.api_node_url)
            .field("bridge_api_url", &self.config.bridge_api_url)
            .field("app_id", &self.config.app_id)
            .field("timeout", &self.config.timeout)
            .field("max_retries", &self.config.max_retries)
            .finish()
    }
}
