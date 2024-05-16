use std::sync::Arc;

use axum::{
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use zksync_config::configs::ProofDataHandlerConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal, SqlxError};
use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_prover_interface::api::{
    GenericProofGenerationDataResponse, SubmitProofRequest, SubmitProofResponse,
    TeeProofGenerationDataRequest,
};
use zksync_tee_verifier::TeeVerifierInput;
use zksync_types::{commitment::serialize_commitments, web3::keccak256, L1BatchNumber, H256};

pub type TeeProofGenerationDataResponse = GenericProofGenerationDataResponse<TeeVerifierInput>;

#[derive(Clone)]
pub(crate) struct TeeRequestProcessor {
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Core>,
    config: ProofDataHandlerConfig,
}

pub(crate) enum TeeRequestProcessorError {
    ObjectStore(ObjectStoreError),
    Sqlx(SqlxError),
}

impl IntoResponse for TeeRequestProcessorError {
    fn into_response(self) -> Response {
        let (status_code, message) = match self {
            TeeRequestProcessorError::ObjectStore(err) => {
                tracing::error!("GCS error: {:?}", err);
                (
                    StatusCode::BAD_GATEWAY,
                    "Failed fetching/saving from GCS".to_owned(),
                )
            }
            TeeRequestProcessorError::Sqlx(err) => {
                tracing::error!("Sqlx error: {:?}", err);
                match err {
                    SqlxError::RowNotFound => {
                        (StatusCode::NOT_FOUND, "Non existing L1 batch".to_owned())
                    }
                    _ => (
                        StatusCode::BAD_GATEWAY,
                        "Failed fetching/saving from db".to_owned(),
                    ),
                }
            }
        };
        (status_code, message).into_response()
    }
}

impl TeeRequestProcessor {
    pub(crate) fn new(
        blob_store: Arc<dyn ObjectStore>,
        pool: ConnectionPool<Core>,
        config: ProofDataHandlerConfig,
    ) -> Self {
        Self {
            blob_store,
            pool,
            config,
        }
    }

    pub(crate) async fn get_proof_generation_data(
        &self,
        request: Json<TeeProofGenerationDataRequest>,
    ) -> Result<Json<TeeProofGenerationDataResponse>, TeeRequestProcessorError> {
        tracing::info!("Received request for proof generation data: {:?}", request);

        // TODO: Replace this line with an appropriate method to get the next batch number to be proven.
        // It's likely that a new SQL column needs to be added to the `proof_generation_details` table.
        // Take self.config.proof_generation_timeout() into account when selecting the next batch to be proven!
        let l1_batch_number_result = Option::from(L1BatchNumber::from(1));

        let l1_batch_number = match l1_batch_number_result {
            Some(number) => number,
            None => return Ok(Json(TeeProofGenerationDataResponse::Success(None))),
        };

        let tee_verifier_input: TeeVerifierInput = self
            .blob_store
            .get(l1_batch_number)
            .await
            .map_err(TeeRequestProcessorError::ObjectStore)?;

        Ok(Json(TeeProofGenerationDataResponse::Success(Some(
            Box::new(tee_verifier_input),
        ))))
    }

    pub(crate) async fn submit_proof(
        &self,
        Path(l1_batch_number): Path<u32>,
        Json(payload): Json<SubmitProofRequest>,
    ) -> Result<Json<SubmitProofResponse>, TeeRequestProcessorError> {
        tracing::info!("Received proof for block number: {:?}", l1_batch_number);
        let l1_batch_number = L1BatchNumber(l1_batch_number);
        match payload {
            SubmitProofRequest::Proof(proof) => {
                let blob_url = self
                    .blob_store
                    .put(l1_batch_number, &*proof)
                    .await
                    .map_err(TeeRequestProcessorError::ObjectStore)?;

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
                    .map_err(TeeRequestProcessorError::Sqlx)?;
            }
            SubmitProofRequest::TeeProof(_proof) => { /* TBD */ }
            SubmitProofRequest::SkippedProofGeneration => {
                self.pool
                    .connection()
                    .await
                    .unwrap()
                    .proof_generation_dal()
                    .mark_proof_generation_job_as_skipped(l1_batch_number)
                    .await
                    .map_err(TeeRequestProcessorError::Sqlx)?;
            }
        }

        Ok(Json(SubmitProofResponse::Success))
    }
}
