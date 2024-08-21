use std::sync::Arc;

use axum::{extract::Path, Json};
use zksync_config::configs::ProofDataHandlerConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_prover_interface::api::{
    RegisterTeeAttestationRequest, RegisterTeeAttestationResponse, SubmitProofResponse,
    SubmitTeeProofRequest, TeeProofGenerationDataRequest, TeeProofGenerationDataResponse,
};
use zksync_types::{tee_types::TeeType, L1BatchNumber};

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

        let mut min_batch_number: Option<L1BatchNumber> = None;

        loop {
            let l1_batch_number = match self
                .lock_batch_for_proving(request.tee_type, min_batch_number)
                .await?
            {
                Some(number) => number,
                None => return Ok(Json(TeeProofGenerationDataResponse(None))),
            };

            match self.blob_store.get(l1_batch_number).await {
                Ok(input) => {
                    return Ok(Json(TeeProofGenerationDataResponse(Some(Box::new(input)))));
                }
                Err(ObjectStoreError::KeyNotFound(_)) => {
                    tracing::warn!(
                        "Blob for batch number {} has not been found in the object store.",
                        l1_batch_number
                    );
                    self.unlock_batch(l1_batch_number, request.tee_type).await?;
                    min_batch_number =
                        min_batch_number.map_or(Some(l1_batch_number + 1), |num| Some(num + 1));
                    continue;
                }
                Err(err) => {
                    self.unlock_batch(l1_batch_number, request.tee_type).await?;
                    return Err(RequestProcessorError::ObjectStore(err));
                }
            }
        }
    }

    async fn lock_batch_for_proving(
        &self,
        tee_type: TeeType,
        min_batch_number: Option<L1BatchNumber>,
    ) -> Result<Option<L1BatchNumber>, RequestProcessorError> {
        self.pool
            .connection()
            .await
            .map_err(RequestProcessorError::Dal)?
            .tee_proof_generation_dal()
            .lock_batch_for_proving(
                tee_type,
                self.config.proof_generation_timeout(),
                min_batch_number,
            )
            .await
            .map_err(RequestProcessorError::Dal)
    }

    async fn unlock_batch(
        &self,
        l1_batch_number: L1BatchNumber,
        tee_type: TeeType,
    ) -> Result<(), RequestProcessorError> {
        self.pool
            .connection()
            .await
            .map_err(RequestProcessorError::Dal)?
            .tee_proof_generation_dal()
            .unlock_batch(l1_batch_number, tee_type)
            .await
            .map_err(RequestProcessorError::Dal)
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
