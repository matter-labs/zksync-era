use axum::extract::Path;
use axum::response::Response;
use axum::{http::StatusCode, response::IntoResponse, Json};
use std::convert::TryFrom;
use std::sync::Arc;
use zksync_config::configs::{
    proof_data_handler::ProtocolVersionLoadingMode, ProofDataHandlerConfig,
};

use zksync_dal::{ConnectionPool, SqlxError};
use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_types::protocol_version::FriProtocolVersionId;
use zksync_types::{
    protocol_version::L1VerifierConfig,
    prover_server_api::{
        ProofGenerationData, ProofGenerationDataRequest, ProofGenerationDataResponse,
        SubmitProofRequest, SubmitProofResponse,
    },
    L1BatchNumber,
};

#[derive(Clone)]
pub(crate) struct RequestProcessor {
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool,
    config: ProofDataHandlerConfig,
    l1_verifier_config: Option<L1VerifierConfig>,
}

pub(crate) enum RequestProcessorError {
    NoPendingBatches,
    ObjectStore(ObjectStoreError),
    Sqlx(SqlxError),
}

impl IntoResponse for RequestProcessorError {
    fn into_response(self) -> Response {
        let (status_code, message) = match self {
            Self::NoPendingBatches => (
                StatusCode::NOT_FOUND,
                "No pending batches to process".to_owned(),
            ),
            RequestProcessorError::ObjectStore(err) => {
                tracing::error!("GCS error: {:?}", err);
                (
                    StatusCode::BAD_GATEWAY,
                    "Failed fetching/saving from GCS".to_owned(),
                )
            }
            RequestProcessorError::Sqlx(err) => {
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

impl RequestProcessor {
    pub(crate) fn new(
        blob_store: Box<dyn ObjectStore>,
        pool: ConnectionPool,
        config: ProofDataHandlerConfig,
        l1_verifier_config: Option<L1VerifierConfig>,
    ) -> Self {
        Self {
            blob_store: Arc::from(blob_store),
            pool,
            config,
            l1_verifier_config,
        }
    }

    pub(crate) async fn get_proof_generation_data(
        &self,
        request: Json<ProofGenerationDataRequest>,
    ) -> Result<Json<ProofGenerationDataResponse>, RequestProcessorError> {
        tracing::info!("Received request for proof generation data: {:?}", request);

        let l1_batch_number = self
            .pool
            .access_storage()
            .await
            .unwrap()
            .proof_generation_dal()
            .get_next_block_to_be_proven(self.config.proof_generation_timeout())
            .await
            .ok_or(RequestProcessorError::NoPendingBatches)?;

        let blob = self
            .blob_store
            .get(l1_batch_number)
            .await
            .map_err(RequestProcessorError::ObjectStore)?;

        let fri_protocol_version_id =
            FriProtocolVersionId::try_from(self.config.fri_protocol_version_id)
                .expect("Invalid FRI protocol version id");

        let l1_verifier_config= match self.config.protocol_version_loading_mode {
            ProtocolVersionLoadingMode::FromDb => {
                panic!("Loading protocol version from db is not implemented yet")
            }
            ProtocolVersionLoadingMode::FromEnvVar => {
                self.l1_verifier_config
                    .expect("l1_verifier_config must be set while running ProtocolVersionLoadingMode::FromEnvVar mode")
            }
        };

        let proof_gen_data = ProofGenerationData {
            l1_batch_number,
            data: blob,
            fri_protocol_version_id,
            l1_verifier_config,
        };

        Ok(Json(ProofGenerationDataResponse::Success(proof_gen_data)))
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
                    .put(l1_batch_number, &*proof)
                    .await
                    .map_err(RequestProcessorError::ObjectStore)?;

                let mut storage = self.pool.access_storage().await.unwrap();
                storage
                    .proof_generation_dal()
                    .save_proof_artifacts_metadata(l1_batch_number, &blob_url)
                    .await
                    .map_err(RequestProcessorError::Sqlx)?;
            }
            SubmitProofRequest::SkippedProofGeneration => {
                self.pool
                    .access_storage()
                    .await
                    .unwrap()
                    .proof_generation_dal()
                    .mark_proof_generation_job_as_skipped(l1_batch_number)
                    .await
                    .map_err(RequestProcessorError::Sqlx)?;
            }
        }

        Ok(Json(SubmitProofResponse::Success))
    }
}
