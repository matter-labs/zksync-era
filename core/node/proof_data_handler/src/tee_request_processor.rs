use std::sync::Arc;

use crate::errors::RequestProcessorError;
use axum::{extract::Path, Json};
use zksync_config::configs::ProofDataHandlerConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_object_store::ObjectStore;
use zksync_prover_interface::api::{
    GenericProofGenerationDataResponse, SubmitProofResponse, SubmitTeeProofRequest,
    TeeProofGenerationDataRequest, TeeType,
};
use zksync_tee_verifier::TeeVerifierInput;
use zksync_types::L1BatchNumber;

pub type TeeProofGenerationDataResponse = GenericProofGenerationDataResponse<TeeVerifierInput>;

#[derive(Clone)]
pub(crate) struct TeeRequestProcessor {
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Core>,
    config: ProofDataHandlerConfig,
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
    ) -> Result<Json<TeeProofGenerationDataResponse>, RequestProcessorError> {
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
            .map_err(RequestProcessorError::ObjectStore)?;

        Ok(Json(TeeProofGenerationDataResponse::Success(Some(
            Box::new(tee_verifier_input),
        ))))
    }

    pub(crate) async fn submit_proof(
        &self,
        Path(tee_type): Path<TeeType>,
        Path(l1_batch_number): Path<u32>,
        Json(payload): Json<SubmitTeeProofRequest>,
    ) -> Result<Json<SubmitProofResponse>, RequestProcessorError> {
        tracing::info!("Received proof for block number: {:?}", l1_batch_number);
        assert_eq!(tee_type, TeeType::Sgx, "Expected TEE type to be SGX");

        let l1_batch_number = L1BatchNumber(l1_batch_number);
        let mut connection = self.pool.connection().await.unwrap();
        let mut dal = connection.proof_generation_dal();

        // TODO: Replace the lines below with code that saves the proof generation result back to the database.
        match payload {
            SubmitTeeProofRequest::Proof(_proof) => {
                // dal.save_proof_artifacts_metadata(l1_batch_number, &blob_url)
                //     .await
                //     .map_err(RequestProcessorError::Sqlx)?;
            }
            SubmitTeeProofRequest::SkippedProofGeneration => {
                dal.mark_proof_generation_job_as_skipped(l1_batch_number)
                    .await
                    .map_err(RequestProcessorError::Sqlx)?;
            }
        }

        Ok(Json(SubmitProofResponse::Success))
    }
}
