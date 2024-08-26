use std::sync::Arc;

use axum::{extract::Path, Json};
use zksync_basic_types::{
    basic_fri_types::Eip4844Blobs, commitment::L1BatchCommitmentMode, L1BatchNumber,
};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_object_store::{bincode, ObjectStore};
use zksync_prover_interface::{
    api::{
        OptionalProofGenerationDataRequest, ProofGenerationData, ProofGenerationDataResponse,
        VerifyProofRequest,
    },
    inputs::{
        L1BatchMetadataHashes, VMRunWitnessInputData, WitnessInputData, WitnessInputMerklePaths,
    },
    outputs::L1BatchProofForL1,
};

use crate::{error::ProcessorError, metrics::Method};

#[derive(Clone)]
pub(crate) struct Processor {
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Core>,
    commitment_mode: L1BatchCommitmentMode,
}

impl Processor {
    pub(crate) fn new(
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
        Path(l1_batch_number): Path<u32>,
        Json(payload): Json<VerifyProofRequest>,
    ) -> Result<(), ProcessorError> {
        let l1_batch_number = L1BatchNumber(l1_batch_number);
        tracing::info!(
            "Received request to verify proof for batch: {:?}",
            l1_batch_number
        );

        let serialized_proof = bincode::serialize(&payload.0)?;
        let expected_proof = bincode::serialize(
            &self
                .blob_store
                .get::<L1BatchProofForL1>((l1_batch_number, payload.0.protocol_version))
                .await?,
        )?;

        if serialized_proof != expected_proof {
            return Err(ProcessorError::InvalidProof);
        }

        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub(crate) async fn get_proof_generation_data(
        &mut self,
        request: Json<OptionalProofGenerationDataRequest>,
    ) -> Result<Json<ProofGenerationDataResponse>, ProcessorError> {
        tracing::info!("Received request for proof generation data: {:?}", request);

        let latest_available_batch = self
            .pool
            .connection()
            .await
            .unwrap()
            .proof_generation_dal()
            .get_latest_proven_batch()
            .await?;

        let l1_batch_number = if let Some(l1_batch_number) = request.0 .0 {
            if l1_batch_number > latest_available_batch {
                tracing::error!(
                    "Requested batch is not available: {:?}, latest available batch is {:?}",
                    l1_batch_number,
                    latest_available_batch
                );
                return Err(ProcessorError::BatchNotReady(l1_batch_number));
            }
            l1_batch_number
        } else {
            latest_available_batch
        };

        let proof_generation_data = self
            .proof_generation_data_for_existing_batch(l1_batch_number)
            .await;

        match proof_generation_data {
            Ok(data) => Ok(Json(ProofGenerationDataResponse::Success(Some(Box::new(
                data,
            ))))),
            Err(err) => Err(err),
        }
    }

    #[tracing::instrument(skip(self))]
    async fn proof_generation_data_for_existing_batch(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<ProofGenerationData, ProcessorError> {
        let vm_run_data: VMRunWitnessInputData = self
            .blob_store
            .get(l1_batch_number)
            .await
            .map_err(ProcessorError::ObjectStore)?;
        let merkle_paths: WitnessInputMerklePaths = self
            .blob_store
            .get(l1_batch_number)
            .await
            .map_err(ProcessorError::ObjectStore)?;

        // Acquire connection after interacting with GCP, to avoid holding the connection for too long.
        let mut conn = self.pool.connection().await.map_err(ProcessorError::Dal)?;

        let previous_batch_metadata = conn
            .blocks_dal()
            .get_l1_batch_metadata(L1BatchNumber(l1_batch_number.checked_sub(1).unwrap()))
            .await
            .map_err(ProcessorError::Dal)?
            .expect("No metadata for previous batch");

        let header = conn
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await
            .map_err(ProcessorError::Dal)?
            .unwrap_or_else(|| panic!("Missing header for {}", l1_batch_number));

        let minor_version = header.protocol_version.unwrap();
        let protocol_version = conn
            .protocol_versions_dal()
            .get_protocol_version_with_latest_patch(minor_version)
            .await
            .map_err(ProcessorError::Dal)?
            .unwrap_or_else(|| {
                panic!("Missing l1 verifier info for protocol version {minor_version}")
            });

        let batch_header = conn
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await
            .map_err(ProcessorError::Dal)?
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
