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
    inputs::{PrepareBasicCircuitsJob, VMRunWitnessInputData, WitnessInputData},
};
use zksync_types::{
    basic_fri_types::Eip4844Blobs,
    commitment::{serialize_commitments, L1BatchCommitmentMode},
    web3::keccak256,
    L1BatchNumber, H256,
};

use crate::errors::RequestProcessorError;

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

    pub(crate) async fn get_proof_generation_data(
        &self,
        request: Json<ProofGenerationDataRequest>,
    ) -> Result<Json<ProofGenerationDataResponse>, RequestProcessorError> {
        tracing::info!("Received request for proof generation data: {:?}", request);

        let l1_batch_number_result = self
            .pool
            .connection()
            .await
            .unwrap()
            .proof_generation_dal()
            .get_next_block_to_be_proven(self.config.proof_generation_timeout())
            .await
            .map_err(RequestProcessorError::Dal)?;

        let l1_batch_number = match l1_batch_number_result {
            Some(number) => number,
            None => return Ok(Json(ProofGenerationDataResponse::Success(None))), // no batches pending to be proven
        };

        let vm_run_data = self
            .blob_store
            .get(l1_batch_number)
            .await
            .map_err(RequestProcessorError::ObjectStore)?;
        let merkle_paths = self
            .blob_store
            .get(l1_batch_number)
            .await
            .map_err(RequestProcessorError::ObjectStore)?;

        let blob = WitnessInputData {
            vm_run_data,
            merkle_paths,
        };

        let header = self
            .pool
            .connection()
            .await
            .unwrap()
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("Missing header for {}", l1_batch_number));

        let minor_version = header.protocol_version.unwrap();
        let protocol_version = self
            .pool
            .connection()
            .await
            .unwrap()
            .protocol_versions_dal()
            .get_protocol_version_with_latest_patch(minor_version)
            .await
            .unwrap()
            .unwrap_or_else(|| {
                panic!("Missing l1 verifier info for protocol version {minor_version}")
            });

        let batch_header = self
            .pool
            .connection()
            .await
            .unwrap()
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await
            .unwrap()
            .unwrap();

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

        let proof_gen_data = ProofGenerationData {
            l1_batch_number,
            data: blob,
            protocol_version: protocol_version.version,
            l1_verifier_config: protocol_version.l1_verifier_config,
            eip_4844_blobs,
        };
        Ok(Json(ProofGenerationDataResponse::Success(Some(Box::new(
            proof_gen_data,
        )))))
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
