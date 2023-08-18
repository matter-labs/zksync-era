use std::sync::Arc;
use std::time::Duration;

use axum::extract::Path;
use axum::response::Response;
use axum::{http::StatusCode, response::IntoResponse, Json};

use zksync_dal::{ConnectionPool, SqlxError};
use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_types::prover_server_api::{
    ProofGenerationData, ProofGenerationDataRequest, ProofGenerationDataResponse,
    SubmitProofRequest, SubmitProofResponse,
};
use zksync_types::L1BatchNumber;

#[derive(Clone)]
pub(crate) struct RequestProcessor {
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool,
    proof_generation_timeout: Duration,
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
                vlog::error!("GCS error: {:?}", err);
                (
                    StatusCode::BAD_GATEWAY,
                    "Failed fetching/saving from GCS".to_owned(),
                )
            }
            RequestProcessorError::Sqlx(err) => {
                vlog::error!("Sqlx error: {:?}", err);
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
        proof_generation_timeout: Duration,
    ) -> Self {
        Self {
            blob_store: Arc::from(blob_store),
            pool,
            proof_generation_timeout,
        }
    }

    pub(crate) async fn get_proof_generation_data(
        &self,
        request: Json<ProofGenerationDataRequest>,
    ) -> Result<Json<ProofGenerationDataResponse>, RequestProcessorError> {
        vlog::info!("Received request for proof generation data: {:?}", request);

        let l1_batch_number = self
            .pool
            .access_storage()
            .await
            .proof_generation_dal()
            .get_next_block_to_be_proven(self.proof_generation_timeout)
            .await
            .ok_or(RequestProcessorError::NoPendingBatches)?;

        let blob = self
            .blob_store
            .get(l1_batch_number)
            .await
            .map_err(RequestProcessorError::ObjectStore)?;

        let proof_gen_data = ProofGenerationData {
            l1_batch_number,
            data: blob,
        };

        Ok(Json(ProofGenerationDataResponse::Success(proof_gen_data)))
    }

    pub(crate) async fn submit_proof(
        &self,
        Path(l1_batch_number): Path<u32>,
        Json(payload): Json<SubmitProofRequest>,
    ) -> Result<Json<SubmitProofResponse>, RequestProcessorError> {
        vlog::info!("Received proof for block number: {:?}", l1_batch_number);
        let l1_batch_number = L1BatchNumber(l1_batch_number);

        let blob_url = self
            .blob_store
            .put(l1_batch_number, &payload.proof)
            .await
            .map_err(RequestProcessorError::ObjectStore)?;

        let mut storage = self.pool.access_storage().await;
        storage
            .proof_generation_dal()
            .save_proof_artifacts_metadata(l1_batch_number, &blob_url)
            .await
            .map_err(RequestProcessorError::Sqlx)?;

        Ok(Json(SubmitProofResponse::Success))
    }
}
