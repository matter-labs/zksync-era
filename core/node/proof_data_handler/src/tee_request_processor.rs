use std::sync::Arc;

use axum::{extract::Path, Json};
use zksync_config::configs::ProofDataHandlerConfig;
use zksync_dal::{tee_proof_generation_dal::TeeType, ConnectionPool, Core, CoreDal};
use zksync_object_store::ObjectStore;
use zksync_prover_interface::api::{
    GenericProofGenerationDataResponse, SubmitProofResponse, SubmitTeeProofRequest,
    TeeProofGenerationDataRequest,
};
use zksync_tee_verifier::TeeVerifierInput;
use zksync_types::L1BatchNumber;

use crate::errors::RequestProcessorError;

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

        let l1_batch_number_result = self
            .pool
            .connection()
            .await
            .unwrap()
            .tee_proof_generation_dal()
            .get_next_block_to_be_proven(self.config.proof_generation_timeout())
            .await;

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
        Path(l1_batch_number): Path<u32>,
        Json(payload): Json<SubmitTeeProofRequest>,
    ) -> Result<Json<SubmitProofResponse>, RequestProcessorError> {
        tracing::info!("Received proof for block number: {:?}", l1_batch_number);

        let l1_batch_number = L1BatchNumber(l1_batch_number);
        let mut connection = self.pool.connection().await.unwrap();
        let mut dal = connection.tee_proof_generation_dal();

        match payload {
            SubmitTeeProofRequest::Proof(proof) => {
                println!(
                    "Received proof {:?} for block number: {:?}",
                    proof, l1_batch_number
                );
                dal.save_proof_artifacts_metadata(
                    l1_batch_number,
                    &proof.signature,
                    &proof.signature,
                    &proof.attestation,
                    TeeType::Sgx,
                )
                .await
                .map_err(RequestProcessorError::Sqlx)?;
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
