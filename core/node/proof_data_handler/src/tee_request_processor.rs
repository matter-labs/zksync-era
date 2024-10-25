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
    inputs::{
        TeeVerifierInput, V1TeeVerifierInput, VMRunWitnessInputData, WitnessInputMerklePaths,
    },
};
use zksync_types::{tee_types::TeeType, L1BatchNumber, L2ChainId};
use zksync_vm_executor::storage::L1BatchParamsProvider;

use crate::errors::RequestProcessorError;

#[derive(Clone)]
pub(crate) struct TeeRequestProcessor {
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Core>,
    config: ProofDataHandlerConfig,
    l2_chain_id: L2ChainId,
}

impl TeeRequestProcessor {
    pub(crate) fn new(
        blob_store: Arc<dyn ObjectStore>,
        pool: ConnectionPool<Core>,
        config: ProofDataHandlerConfig,
        l2_chain_id: L2ChainId,
    ) -> Self {
        Self {
            blob_store,
            pool,
            config,
            l2_chain_id,
        }
    }

    pub(crate) async fn get_proof_generation_data(
        &self,
        request: Json<TeeProofGenerationDataRequest>,
    ) -> Result<Option<Json<TeeProofGenerationDataResponse>>, RequestProcessorError> {
        tracing::info!("Received request for proof generation data: {:?}", request);

        let mut min_batch_number = self.config.tee_config.first_tee_processed_batch;
        let mut missing_range: Option<(L1BatchNumber, L1BatchNumber)> = None;

        let result = loop {
            let Some(l1_batch_number) = self
                .lock_batch_for_proving(request.tee_type, min_batch_number)
                .await?
            else {
                // No job available
                return Ok(None);
            };

            match self
                .tee_verifier_input_for_existing_batch(l1_batch_number)
                .await
            {
                Ok(input) => {
                    break Ok(Some(Json(TeeProofGenerationDataResponse(Box::new(input)))));
                }
                Err(RequestProcessorError::ObjectStore(ObjectStoreError::KeyNotFound(_))) => {
                    missing_range = match missing_range {
                        Some((start, _)) => Some((start, l1_batch_number)),
                        None => Some((l1_batch_number, l1_batch_number)),
                    };
                    self.unlock_batch(l1_batch_number, request.tee_type).await?;
                    min_batch_number = l1_batch_number + 1;
                }
                Err(err) => {
                    self.unlock_batch(l1_batch_number, request.tee_type).await?;
                    break Err(err);
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

    #[tracing::instrument(skip(self))]
    async fn tee_verifier_input_for_existing_batch(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<TeeVerifierInput, RequestProcessorError> {
        let vm_run_data: VMRunWitnessInputData = self
            .blob_store
            .get(l1_batch_number)
            .await
            .map_err(RequestProcessorError::ObjectStore)?;

        let merkle_paths: WitnessInputMerklePaths = self
            .blob_store
            .get(l1_batch_number)
            .await
            .map_err(RequestProcessorError::ObjectStore)?;

        let mut connection = self
            .pool
            .connection_tagged("tee_request_processor")
            .await
            .map_err(RequestProcessorError::Dal)?;

        let l2_blocks_execution_data = connection
            .transactions_dal()
            .get_l2_blocks_to_execute_for_l1_batch(l1_batch_number)
            .await
            .map_err(RequestProcessorError::Dal)?;

        let l1_batch_params_provider = L1BatchParamsProvider::new(&mut connection)
            .await
            .map_err(|err| RequestProcessorError::GeneralError(err.to_string()))?;

        // In the state keeper, this value is used to reject execution.
        // All batches have already been executed by State Keeper.
        // This means we don't want to reject any execution, therefore we're using MAX as an allow all.
        let validation_computational_gas_limit = u32::MAX;

        let (system_env, l1_batch_env, pubdata_params) = l1_batch_params_provider
            .load_l1_batch_env(
                &mut connection,
                l1_batch_number,
                validation_computational_gas_limit,
                self.l2_chain_id,
            )
            .await
            .map_err(|err| RequestProcessorError::GeneralError(err.to_string()))?
            .ok_or(RequestProcessorError::GeneralError(
                "system_env, l1_batch_env missing".into(),
            ))?;

        Ok(TeeVerifierInput::new(V1TeeVerifierInput {
            vm_run_data,
            merkle_paths,
            l2_blocks_execution_data,
            l1_batch_env,
            system_env,
            pubdata_params,
        }))
    }

    async fn lock_batch_for_proving(
        &self,
        tee_type: TeeType,
        min_batch_number: L1BatchNumber,
    ) -> Result<Option<L1BatchNumber>, RequestProcessorError> {
        self.pool
            .connection_tagged("tee_request_processor")
            .await?
            .tee_proof_generation_dal()
            .lock_batch_for_proving(
                tee_type,
                self.config.tee_config.tee_proof_generation_timeout(),
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
            .connection_tagged("tee_request_processor")
            .await?
            .tee_proof_generation_dal()
            .unlock_batch(l1_batch_number, tee_type)
            .await?;
        Ok(())
    }

    pub(crate) async fn submit_proof(
        &self,
        Path(l1_batch_number): Path<u32>,
        Json(proof): Json<SubmitTeeProofRequest>,
    ) -> Result<Json<SubmitProofResponse>, RequestProcessorError> {
        let l1_batch_number = L1BatchNumber(l1_batch_number);
        let mut connection = self.pool.connection_tagged("tee_request_processor").await?;
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

        let mut connection = self.pool.connection_tagged("tee_request_processor").await?;
        let mut dal = connection.tee_proof_generation_dal();

        dal.save_attestation(&payload.pubkey, &payload.attestation)
            .await?;

        Ok(Json(RegisterTeeAttestationResponse::Success))
    }
}
