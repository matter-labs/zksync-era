use anyhow::anyhow;
use async_trait::async_trait;
use std::{fmt::Debug, sync::Arc};

use near_primitives::borsh;

use zksync_da_client::{
    types::{self, DAError},
    DataAvailabilityClient,
};

use crate::{
    near::{
        sdk::{verify_proof, DAClient},
        types::BlobInclusionProof,
    },
    utils::to_non_retriable_da_error,
};

use zksync_config::NearConfig;

#[derive(Clone)]
pub struct NearClient {
    pub(crate) config: NearConfig,
    pub(crate) da_rpc_client: Arc<DAClient>,
}

impl Debug for NearClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NearClient")
            .field("config", &self.config)
            .field("client", &self.da_rpc_client)
            .finish()
    }
}

impl NearClient {
    pub async fn new(config: NearConfig) -> anyhow::Result<Self> {
        let da_rpc_client = DAClient::new(&config)
            .await
            .map_err(to_non_retriable_da_error)?;

        Ok(Self {
            config,
            da_rpc_client: Arc::new(da_rpc_client),
        })
    }
}

#[async_trait]
impl DataAvailabilityClient for NearClient {
    async fn dispatch_blob(
        &self,
        _batch_number: u32,
        data: Vec<u8>,
    ) -> Result<types::DispatchResponse, types::DAError> {
        let blob_hash = self.da_rpc_client.submit(data).await?;

        let blob_id = blob_hash.to_string();

        Ok(types::DispatchResponse { blob_id })
    }

    async fn get_inclusion_data(
        &self,
        blob_id: &str,
    ) -> Result<Option<types::InclusionData>, types::DAError> {
        // Call bridge_contract `latestHeader` method to get the latest ZK-verified block header hash
        // let latest_header = self.da_rpc_client.latest_header(&self.config).await?;
        // let latest_header_view = self.da_rpc_client.get_header(latest_header).await?;
        // let latest_header_hash = latest_header_view.hash();

        // if latest_header_hash != latest_header {
        //     return Err(DAError {
        //         error: anyhow!("Light client header mismatch"),
        //         is_retriable: false,
        //     });
        // }

        // let proof = self.da_rpc_client.get_proof(blob_id, latest_header).await?;
        // let head_block_root = latest_header_view.inner_lite.block_merkle_root;

        // verify_proof(head_block_root, &proof)?;

        // let attestation_data = BlobInclusionProof {
        //     // outcome_proof and outcome_root_proof are used to
        //     // calculate and validate the block outcome_root
        //     outcome_proof: proof
        //         .outcome_proof
        //         .try_into()
        //         .map_err(to_non_retriable_da_error)?,
        //     outcome_root_proof: proof.outcome_root_proof,
        //     // block_header_lite and block_proof are used
        //     // to calculate and validate the block merkle root against the latest ZK-verified block header merkle root
        //     block_header_lite: proof
        //         .block_header_lite
        //         .try_into()
        //         .map_err(to_non_retriable_da_error)?,
        //     block_proof: proof.block_proof,
        //     // head_merkle_root is the latest ZK-verified block header merkle root which should be reproduced
        //     // by calculating the merkle root from the block_proof and block_header_lite
        //     // recieved from fetching the inclusion proof
        //     head_merkle_root: head_block_root.0,
        // };

        // Ok(Some(types::InclusionData {
        //     data: borsh::to_vec(&attestation_data).map_err(to_non_retriable_da_error)?,
        // }))

        Ok(Some(types::InclusionData { data: vec![] }))
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> std::option::Option<usize> {
        Some(1572864)
    }
}
