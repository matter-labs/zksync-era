use std::sync::Arc;

use zksync_basic_types::{
    basic_fri_types::Eip4844Blobs, commitment::L1BatchCommitmentMode, L1BatchNumber,
};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_object_store::ObjectStore;
use zksync_prover_interface::{
    api::ProofGenerationData,
    inputs::{
        L1BatchMetadataHashes, VMRunWitnessInputData, WitnessInputData, WitnessInputMerklePaths,
    },
    outputs::L1BatchProofForL1,
};

use crate::{
    error::ProcessorError,
    types::{ExternalProof, ProofGenerationDataResponse},
};

/// Backend-agnostic implementation of the API logic.
#[derive(Clone)]
pub struct Processor {
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Core>,
    commitment_mode: L1BatchCommitmentMode,
}

impl Processor {
    pub fn new(
        blob_store: Arc<dyn ObjectStore>,
        pool: ConnectionPool<Core>,
        commitment_mode: L1BatchCommitmentMode,
    ) -> Self {
        Self {
            blob_store,
            pool,
            commitment_mode,
        }
    }

    pub(crate) async fn verify_proof(
        &self,
        l1_batch_number: L1BatchNumber,
        proof: ExternalProof,
    ) -> Result<(), ProcessorError> {
        let expected_proof = self
            .blob_store
            .get::<L1BatchProofForL1>((l1_batch_number, proof.protocol_version()))
            .await?;
        proof.verify(expected_proof)?;
        Ok(())
    }

    pub(crate) async fn get_proof_generation_data(
        &self,
    ) -> Result<ProofGenerationDataResponse, ProcessorError> {
        tracing::debug!("Received request for proof generation data");
        let latest_available_batch = self.latest_available_batch().await?;
        self.proof_generation_data_for_existing_batch_internal(latest_available_batch)
            .await
            .map(ProofGenerationDataResponse)
    }

    pub(crate) async fn proof_generation_data_for_existing_batch(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<ProofGenerationDataResponse, ProcessorError> {
        tracing::debug!(
            "Received request for proof generation data for batch: {:?}",
            l1_batch_number
        );

        let latest_available_batch = self.latest_available_batch().await?;

        if l1_batch_number > latest_available_batch {
            tracing::error!(
                "Requested batch is not available: {:?}, latest available batch is {:?}",
                l1_batch_number,
                latest_available_batch
            );
            return Err(ProcessorError::BatchNotReady(l1_batch_number));
        }

        self.proof_generation_data_for_existing_batch_internal(l1_batch_number)
            .await
            .map(ProofGenerationDataResponse)
    }

    async fn latest_available_batch(&self) -> Result<L1BatchNumber, ProcessorError> {
        Ok(self
            .pool
            .connection()
            .await
            .unwrap()
            .proof_generation_dal()
            .get_latest_proven_batch()
            .await?)
    }

    async fn proof_generation_data_for_existing_batch_internal(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<ProofGenerationData, ProcessorError> {
        let vm_run_data: VMRunWitnessInputData = self.blob_store.get(l1_batch_number).await?;
        let merkle_paths: WitnessInputMerklePaths = self.blob_store.get(l1_batch_number).await?;

        // Acquire connection after interacting with GCP, to avoid holding the connection for too long.
        let mut conn = self.pool.connection().await?;

        let previous_batch_metadata = conn
            .blocks_dal()
            .get_l1_batch_metadata(L1BatchNumber(l1_batch_number.checked_sub(1).unwrap()))
            .await?
            .expect("No metadata for previous batch");

        let header = conn
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await?
            .unwrap_or_else(|| panic!("Missing header for {}", l1_batch_number));

        let minor_version = header.protocol_version.unwrap();
        let protocol_version = conn
            .protocol_versions_dal()
            .get_protocol_version_with_latest_patch(minor_version)
            .await?
            .unwrap_or_else(|| {
                panic!("Missing l1 verifier info for protocol version {minor_version}")
            });

        let batch_header = conn
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await?
            .unwrap_or_else(|| panic!("Missing header for {}", l1_batch_number));

        let eip_4844_blobs = match self.commitment_mode {
            L1BatchCommitmentMode::Validium => Eip4844Blobs::empty(),
            L1BatchCommitmentMode::Rollup => {
                let blobs = batch_header.pubdata_input.as_deref().unwrap_or_else(|| {
                    panic!(
                        "expected pubdata, but it is not available for batch {l1_batch_number:?}"
                    )
                });
                Eip4844Blobs::decode(blobs).expect("failed to decode EIP-4844 blobs")
            }
        };

        let blob = WitnessInputData {
            vm_run_data,
            merkle_paths,
            eip_4844_blobs,
            previous_batch_metadata: L1BatchMetadataHashes {
                root_hash: previous_batch_metadata.metadata.root_hash,
                meta_hash: previous_batch_metadata.metadata.meta_parameters_hash,
                aux_hash: previous_batch_metadata.metadata.aux_data_hash,
            },
        };

        Ok(ProofGenerationData {
            l1_batch_number,
            witness_input_data: blob,
            protocol_version: protocol_version.version,
            l1_verifier_config: protocol_version.l1_verifier_config,
        })
    }
}
