use std::sync::Arc;

use axum::{extract::Path, Json};
use zksync_config::configs::ProofDataHandlerConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_object_store::ObjectStore;
use zksync_prover_interface::{
    api::{
        ProofGenerationData, ProofGenerationDataRequest, ProofGenerationDataResponse,
        SubmitProofRequest, SubmitProofResponse,
    },
    inputs::{
        L1BatchMetadataHashes, VMRunWitnessInputData, WitnessInputData, WitnessInputMerklePaths,
    },
};
use zksync_types::{
    basic_fri_types::Eip4844Blobs,
    commitment::{serialize_commitments, L1BatchCommitmentMode},
    web3::keccak256,
    L1BatchNumber, H256,
};

use crate::{errors::RequestProcessorError, metrics::METRICS};

#[derive(Clone)]
pub(crate) struct RequestProcessor {
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Core>,
    config: ProofDataHandlerConfig,
    commitment_mode: L1BatchCommitmentMode,
}

impl RequestProcessor {
    pub(crate) fn new(
        blob_store: Arc<dyn ObjectStore>,
        pool: ConnectionPool<Core>,
        config: ProofDataHandlerConfig,
        commitment_mode: L1BatchCommitmentMode,
    ) -> Self {
        Self {
            blob_store,
            pool,
            config,
            commitment_mode,
        }
    }

    #[tracing::instrument(skip_all)]
    pub(crate) async fn get_proof_generation_data(
        &self,
        request: Json<ProofGenerationDataRequest>,
    ) -> Result<Json<ProofGenerationDataResponse>, RequestProcessorError> {
        tracing::info!("Received request for proof generation data: {:?}", request);

        let l1_batch_number = match self.lock_batch_for_proving().await? {
            Some(number) => number,
            None => return Ok(Json(ProofGenerationDataResponse::Success(None))), // no batches pending to be proven
        };

        let proof_generation_data = self
            .proof_generation_data_for_existing_batch(l1_batch_number)
            .await;

        // If we weren't able to fetch all the data, we should unlock the batch before returning.
        match proof_generation_data {
            Ok(data) => Ok(Json(ProofGenerationDataResponse::Success(Some(Box::new(
                data,
            ))))),
            Err(err) => {
                self.unlock_batch(l1_batch_number).await?;
                Err(err)
            }
        }
    }

    /// Will choose a batch that has all the required data and isn't picked up by any prover yet.
    async fn lock_batch_for_proving(&self) -> Result<Option<L1BatchNumber>, RequestProcessorError> {
        self.pool
            .connection()
            .await
            .map_err(RequestProcessorError::Dal)?
            .proof_generation_dal()
            .lock_batch_for_proving(self.config.proof_generation_timeout())
            .await
            .map_err(RequestProcessorError::Dal)
    }

    /// Marks the batch as 'unpicked', allowing it to be picked up by another prover.
    async fn unlock_batch(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<(), RequestProcessorError> {
        self.pool
            .connection()
            .await
            .map_err(RequestProcessorError::Dal)?
            .proof_generation_dal()
            .unlock_batch(l1_batch_number)
            .await
            .map_err(RequestProcessorError::Dal)
    }

    /// Will fetch all the required data for the batch and return it.
    ///
    /// ## Panics
    ///
    /// Expects all the data to be present in the database.
    /// Will panic if any of the required data is missing.
    #[tracing::instrument(skip(self))]
    async fn proof_generation_data_for_existing_batch(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<ProofGenerationData, RequestProcessorError> {
        proof_generation_data_for_existing_batch_impl(
            self.blob_store.clone(),
            self.pool.clone(),
            self.commitment_mode,
            l1_batch_number,
        )
        .await
    }

    pub(crate) async fn submit_proof(
        &self,
        Path(l1_batch_number): Path<u32>,
        Json(payload): Json<SubmitProofRequest>,
    ) -> Result<Json<SubmitProofResponse>, RequestProcessorError> {
        tracing::info!("Received proof for block number: {:?}", l1_batch_number);
        let l1_batch_number = L1BatchNumber(l1_batch_number);
        match payload {
            SubmitProofRequest::Proof(proof) => {
                let blob_url = self
                    .blob_store
                    .put((l1_batch_number, proof.protocol_version), &*proof)
                    .await
                    .map_err(RequestProcessorError::ObjectStore)?;

                let system_logs_hash_from_prover =
                    H256::from_slice(&proof.aggregation_result_coords[0]);
                let state_diff_hash_from_prover =
                    H256::from_slice(&proof.aggregation_result_coords[1]);
                let bootloader_heap_initial_content_from_prover =
                    H256::from_slice(&proof.aggregation_result_coords[2]);
                let events_queue_state_from_prover =
                    H256::from_slice(&proof.aggregation_result_coords[3]);

                let mut storage = self.pool.connection().await.unwrap();

                let l1_batch = storage
                    .blocks_dal()
                    .get_l1_batch_metadata(l1_batch_number)
                    .await
                    .unwrap()
                    .expect("Proved block without metadata");

                let is_pre_boojum = l1_batch
                    .header
                    .protocol_version
                    .map(|v| v.is_pre_boojum())
                    .unwrap_or(true);
                if !is_pre_boojum {
                    let events_queue_state = l1_batch
                        .metadata
                        .events_queue_commitment
                        .expect("No events_queue_commitment");
                    let bootloader_heap_initial_content = l1_batch
                        .metadata
                        .bootloader_initial_content_commitment
                        .expect("No bootloader_initial_content_commitment");

                    if events_queue_state != events_queue_state_from_prover
                        || bootloader_heap_initial_content
                            != bootloader_heap_initial_content_from_prover
                    {
                        let server_values = format!("events_queue_state = {events_queue_state}, bootloader_heap_initial_content = {bootloader_heap_initial_content}");
                        let prover_values = format!("events_queue_state = {events_queue_state_from_prover}, bootloader_heap_initial_content = {bootloader_heap_initial_content_from_prover}");
                        panic!(
                            "Auxilary output doesn't match, server values: {} prover values: {}",
                            server_values, prover_values
                        );
                    }
                }

                let system_logs = serialize_commitments(&l1_batch.header.system_logs);
                let system_logs_hash = H256(keccak256(&system_logs));

                if !is_pre_boojum {
                    let state_diff_hash = l1_batch
                        .header
                        .system_logs
                        .into_iter()
                        .find(|elem| elem.0.key == H256::from_low_u64_be(2))
                        .expect("No state diff hash key")
                        .0
                        .value;

                    if state_diff_hash != state_diff_hash_from_prover
                        || system_logs_hash != system_logs_hash_from_prover
                    {
                        let server_values = format!("system_logs_hash = {system_logs_hash}, state_diff_hash = {state_diff_hash}");
                        let prover_values = format!("system_logs_hash = {system_logs_hash_from_prover}, state_diff_hash = {state_diff_hash_from_prover}");
                        panic!(
                            "Auxilary output doesn't match, server values: {} prover values: {}",
                            server_values, prover_values
                        );
                    }
                }
                storage
                    .proof_generation_dal()
                    .save_proof_artifacts_metadata(l1_batch_number, &blob_url)
                    .await
                    .map_err(RequestProcessorError::Dal)?;
            }
            SubmitProofRequest::SkippedProofGeneration => {
                self.pool
                    .connection()
                    .await
                    .unwrap()
                    .proof_generation_dal()
                    .mark_proof_generation_job_as_skipped(l1_batch_number)
                    .await
                    .map_err(RequestProcessorError::Dal)?;
            }
        }

        Ok(Json(SubmitProofResponse::Success))
    }
}

pub async fn proof_generation_data_for_existing_batch_impl(
    blob_store: Arc<dyn ObjectStore>,
    connection_pool: ConnectionPool<Core>,
    commitment_mode: L1BatchCommitmentMode,
    l1_batch_number: L1BatchNumber,
) -> Result<ProofGenerationData, RequestProcessorError> {
    let vm_run_data: VMRunWitnessInputData = blob_store
        .get(l1_batch_number)
        .await
        .map_err(RequestProcessorError::ObjectStore)?;
    let merkle_paths: WitnessInputMerklePaths = blob_store
        .get(l1_batch_number)
        .await
        .map_err(RequestProcessorError::ObjectStore)?;

    // Acquire connection after interacting with GCP, to avoid holding the connection for too long.
    let mut conn = connection_pool
        .connection()
        .await
        .map_err(RequestProcessorError::Dal)?;

    let previous_batch_metadata = conn
        .blocks_dal()
        .get_l1_batch_metadata(L1BatchNumber(l1_batch_number.checked_sub(1).unwrap()))
        .await
        .map_err(RequestProcessorError::Dal)?
        .expect("No metadata for previous batch");

    let header = conn
        .blocks_dal()
        .get_l1_batch_header(l1_batch_number)
        .await
        .map_err(RequestProcessorError::Dal)?
        .unwrap_or_else(|| panic!("Missing header for {}", l1_batch_number));

    let minor_version = header.protocol_version.unwrap();
    let protocol_version = conn
        .protocol_versions_dal()
        .get_protocol_version_with_latest_patch(minor_version)
        .await
        .map_err(RequestProcessorError::Dal)?
        .unwrap_or_else(|| panic!("Missing l1 verifier info for protocol version {minor_version}"));

    let batch_header = conn
        .blocks_dal()
        .get_l1_batch_header(l1_batch_number)
        .await
        .map_err(RequestProcessorError::Dal)?
        .unwrap_or_else(|| panic!("Missing header for {}", l1_batch_number));

    let eip_4844_blobs = match commitment_mode {
        L1BatchCommitmentMode::Validium => Eip4844Blobs::empty(),
        L1BatchCommitmentMode::Rollup => {
            let blobs = batch_header.pubdata_input.as_deref().unwrap_or_else(|| {
                panic!("expected pubdata, but it is not available for batch {l1_batch_number:?}")
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

    METRICS.observe_blob_sizes(&blob);

    Ok(ProofGenerationData {
        l1_batch_number,
        witness_input_data: blob,
        protocol_version: protocol_version.version,
        l1_verifier_config: protocol_version.l1_verifier_config,
    })
}
