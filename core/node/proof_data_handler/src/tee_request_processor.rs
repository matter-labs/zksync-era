use std::sync::Arc;

use axum::{extract::Path, Json};
use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use zksync_config::configs::ProofDataHandlerConfig;
use zksync_dal::{
    tee_proof_generation_dal::TeeProofGenerationJobStatus, ConnectionPool, Core, CoreDal,
};
use zksync_object_store::ObjectStore;
use zksync_prover_interface::api::{
    RegisterTeeAttestationRequest, RegisterTeeAttestationResponse, SubmitProofResponse,
    SubmitTeeProofRequest, TeeProofGenerationDataRequest, TeeProofGenerationDataResponse,
};
use zksync_types::{
    tee_types::{LockedBatch, TeeType},
    L1BatchNumber,
};

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
        let mut missing_range: Option<(L1BatchNumber, L1BatchNumber)> = None;

        let result = loop {
            let locked_batch = match self
                .lock_batch_for_proving(request.tee_type, min_batch_number)
                .await?
            {
                Some(number) => number,
                None => break Ok(Json(TeeProofGenerationDataResponse(None))),
            };
            let batch_number = locked_batch.l1_batch_number;

            match self.blob_store.get_optional(batch_number).await {
                Ok(Some(input)) => {
                    break Ok(Json(TeeProofGenerationDataResponse(Some(Box::new(input)))))
                }
                Ok(None) => {
                    missing_range = match missing_range {
                        Some((start, _)) => Some((start, batch_number)),
                        None => Some((batch_number, batch_number)),
                    };
                    let datetime_utc = Utc.from_utc_datetime(&locked_batch.created_at);
                    let duration = Utc::now().signed_duration_since(datetime_utc);
                    let status = if duration > ChronoDuration::days(10) {
                        TeeProofGenerationJobStatus::PermanentlyIgnored
                    } else {
                        TeeProofGenerationJobStatus::Unpicked
                    };
                    self.unlock_batch(batch_number, request.tee_type, status)
                        .await?;
                    min_batch_number = Some(min_batch_number.unwrap_or(batch_number) + 1);
                }
                Err(err) => {
                    self.unlock_batch(
                        batch_number,
                        request.tee_type,
                        TeeProofGenerationJobStatus::Unpicked,
                    )
                    .await?;
                    break Err(RequestProcessorError::ObjectStore(err));
                }
            }
        };

        if let Some((start, end)) = missing_range {
            tracing::warn!(
                "Blobs for batch numbers {} to {} not found in the object store. Marked as unpicked.",
                start,
                end
            );
        }

        result
    }

    async fn lock_batch_for_proving(
        &self,
        tee_type: TeeType,
        min_batch_number: Option<L1BatchNumber>,
    ) -> Result<Option<LockedBatch>, RequestProcessorError> {
        let locked_batch = self
            .pool
            .connection()
            .await?
            .tee_proof_generation_dal()
            .lock_batch_for_proving(
                tee_type,
                self.config.proof_generation_timeout(),
                min_batch_number,
            )
            .await?;
        Ok(locked_batch)
    }

    async fn unlock_batch(
        &self,
        l1_batch_number: L1BatchNumber,
        tee_type: TeeType,
        status: TeeProofGenerationJobStatus,
    ) -> Result<(), RequestProcessorError> {
        self.pool
            .connection()
            .await?
            .tee_proof_generation_dal()
            .unlock_batch(l1_batch_number, tee_type, status)
            .await?;
        Ok(())
    }

    pub(crate) async fn submit_proof(
        &self,
        Path(l1_batch_number): Path<u32>,
        Json(proof): Json<SubmitTeeProofRequest>,
    ) -> Result<Json<SubmitProofResponse>, RequestProcessorError> {
        let l1_batch_number = L1BatchNumber(l1_batch_number);
        let mut connection = self.pool.connection().await?;
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
        .await?;

        Ok(Json(SubmitProofResponse::Success))
    }

    pub(crate) async fn register_tee_attestation(
        &self,
        Json(payload): Json<RegisterTeeAttestationRequest>,
    ) -> Result<Json<RegisterTeeAttestationResponse>, RequestProcessorError> {
        tracing::info!("Received attestation: {:?}", payload);

        let mut connection = self.pool.connection().await?;
        let mut dal = connection.tee_proof_generation_dal();

        dal.save_attestation(&payload.pubkey, &payload.attestation)
            .await?;

        Ok(Json(RegisterTeeAttestationResponse::Success))
    }
}
