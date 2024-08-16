use std::sync::Arc;

use axum::{extract::Path, Json};
use zksync_config::configs::ProofDataHandlerConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_prover_interface::{
    api::{
        RegisterTeeAttestationRequest, RegisterTeeAttestationResponse, SubmitProofResponse,
        SubmitTeeProofRequest, TeeProofGenerationDataRequest, TeeProofGenerationDataResponse,
    },
    inputs::TeeVerifierInput,
};
use zksync_types::L1BatchNumber;

use crate::errors::RequestProcessorError;

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

        let mut connection = self
            .pool
            .connection()
            .await
            .map_err(RequestProcessorError::Dal)?;

        loop {
            let l1_batch_number = match connection
                .tee_proof_generation_dal()
                .get_next_batch_to_be_proven(
                    request.tee_type,
                    self.config.proof_generation_timeout(),
                )
                .await
                .map_err(RequestProcessorError::Dal)?
            {
                Some(number) => number,
                None => return Ok(Json(TeeProofGenerationDataResponse(None))),
            };

            match self.get_blob(l1_batch_number).await {
                Ok(input) => {
                    return Ok(Json(TeeProofGenerationDataResponse(Some(Box::new(input)))));
                }
                Err(ObjectStoreError::KeyNotFound(_)) => {
                    tracing::warn!(
                        "Blob for batch number {} has not been found in the object store. Marking the job as skipped.",
                        l1_batch_number
                    );
                    connection
                        .tee_proof_generation_dal()
                        .mark_proof_generation_job_as_skipped(l1_batch_number, request.tee_type)
                        .await
                        .map_err(RequestProcessorError::Dal)?;
                    continue;
                }
                Err(err) => return Err(RequestProcessorError::ObjectStore(err)),
            }
        }
    }

    async fn get_blob(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<TeeVerifierInput, ObjectStoreError> {
        let max_blob_store_retries = 3;
        let mut last_err: Option<ObjectStoreError> = None;

        for _ in 0..max_blob_store_retries {
            match self.blob_store.get(l1_batch_number).await {
                Ok(input) => return Ok(input),
                Err(err) => match err {
                    ObjectStoreError::Other { is_retriable, .. } if is_retriable => {
                        last_err = Some(err);
                        continue;
                    }
                    _ => return Err(err),
                },
            }
        }

        Err(last_err.unwrap())
    }

    pub(crate) async fn submit_proof(
        &self,
        Path(l1_batch_number): Path<u32>,
        Json(proof): Json<SubmitTeeProofRequest>,
    ) -> Result<Json<SubmitProofResponse>, RequestProcessorError> {
        let l1_batch_number = L1BatchNumber(l1_batch_number);
        let mut connection = self
            .pool
            .connection()
            .await
            .map_err(RequestProcessorError::Dal)?;
        let mut dal = connection.tee_proof_generation_dal();

        tracing::info!(
            "Received proof {:?} for batch number: {:?}",
            proof,
            l1_batch_number
        );
        dal.save_proof_artifacts_metadata(
            l1_batch_number,
            proof.0.tee_type,
            &proof.0.pubkey,
            &proof.0.signature,
            &proof.0.proof,
        )
        .await
        .map_err(RequestProcessorError::Dal)?;

        Ok(Json(SubmitProofResponse::Success))
    }

    pub(crate) async fn register_tee_attestation(
        &self,
        Json(payload): Json<RegisterTeeAttestationRequest>,
    ) -> Result<Json<RegisterTeeAttestationResponse>, RequestProcessorError> {
        tracing::info!("Received attestation: {:?}", payload);

        let mut connection = self
            .pool
            .connection()
            .await
            .map_err(RequestProcessorError::Dal)?;
        let mut dal = connection.tee_proof_generation_dal();

        dal.save_attestation(&payload.pubkey, &payload.attestation)
            .await
            .map_err(RequestProcessorError::Dal)?;

        Ok(Json(RegisterTeeAttestationResponse::Success))
    }
}
