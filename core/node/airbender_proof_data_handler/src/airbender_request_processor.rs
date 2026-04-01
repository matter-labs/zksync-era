use std::sync::Arc;

use axum::{extract::Path, Json};
use chrono::Utc;
use zksync_airbender_prover_interface::{
    api::{SubmitAirbenderProofRequest, SubmitAirbenderProofResponse},
    encoding::encode_input_to_hex,
    inputs::{AirbenderVerifierInput, V1AirbenderVerifierInput},
    outputs::L1BatchAirbenderProofForL1,
};
use zksync_config::configs::AirbenderProofDataHandlerConfig;
use zksync_dal::{
    airbender_proof_generation_dal::{AirbenderProofGenerationJobStatus, LockedBatch},
    ConnectionPool, Core, CoreDal,
};
use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_prover_interface::inputs::{VMRunWitnessInputData, WitnessInputMerklePaths};
use zksync_types::{L1BatchNumber, L2ChainId};
use zksync_vm_executor::storage::{L1BatchParamsProvider, RestoredL1BatchEnv};

use crate::{errors::AirbenderProcessorError, metrics::METRICS};

#[derive(Clone)]
pub(crate) struct AirbenderRequestProcessor {
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Core>,
    config: AirbenderProofDataHandlerConfig,
    l2_chain_id: L2ChainId,
}

impl AirbenderRequestProcessor {
    pub(crate) fn new(
        blob_store: Arc<dyn ObjectStore>,
        pool: ConnectionPool<Core>,
        config: AirbenderProofDataHandlerConfig,
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
    ) -> Result<Option<String>, AirbenderProcessorError> {
        tracing::info!("Received request for proof generation data");

        let min_batch_number = self.config.first_processed_batch;
        const MAX_ATTEMPTS: usize = 5;

        for attempt in 0..MAX_ATTEMPTS {
            let Some(locked_batch) = self.lock_batch_for_proving(min_batch_number).await? else {
                return Ok(None); // no job available
            };
            let batch_number = locked_batch.l1_batch_number;

            match self
                .airbender_verifier_input_for_existing_batch(batch_number)
                .await
            {
                Ok(input) => {
                    let hex = encode_input_to_hex(&input).map_err(|err| {
                        AirbenderProcessorError::GeneralError(format!(
                            "Failed to encode verifier input for batch {batch_number}: {err}"
                        ))
                    })?;
                    return Ok(Some(hex));
                }
                Err(AirbenderProcessorError::ObjectStore {
                    source: ObjectStoreError::KeyNotFound(_),
                    context,
                }) => {
                    self.unlock_batch(batch_number, AirbenderProofGenerationJobStatus::Failed)
                        .await?;
                    tracing::warn!(
                        "Data not available on GCS for batch {} created at {} (attempt {}/{}): {context}",
                        batch_number,
                        locked_batch.created_at,
                        attempt + 1,
                        MAX_ATTEMPTS,
                    );
                    continue; // try the next batch
                }
                Err(err) => {
                    self.unlock_batch(batch_number, AirbenderProofGenerationJobStatus::Failed)
                        .await?;
                    return Err(err);
                }
            }
        }

        tracing::warn!("Exhausted {MAX_ATTEMPTS} attempts to find a batch with available GCS data");
        Ok(None)
    }

    #[tracing::instrument(skip(self))]
    async fn airbender_verifier_input_for_existing_batch(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<AirbenderVerifierInput, AirbenderProcessorError> {
        let vm_run_data: VMRunWitnessInputData = self
            .blob_store
            .get(l1_batch_number)
            .await
            .map_err(|source| AirbenderProcessorError::ObjectStore {
                source,
                context: "Failed to get VMRunWitnessInputData".into(),
            })?;

        let merkle_paths: WitnessInputMerklePaths = self
            .blob_store
            .get(l1_batch_number)
            .await
            .map_err(|source| AirbenderProcessorError::ObjectStore {
                source,
                context: "Failed to get WitnessInputMerklePaths".into(),
            })?;

        let mut connection = self
            .pool
            .connection_tagged("airbender_request_processor")
            .await?;

        let l2_blocks_execution_data = connection
            .transactions_dal()
            .get_l2_blocks_to_execute_for_l1_batch(l1_batch_number)
            .await?;

        let l1_batch_params_provider = L1BatchParamsProvider::new(&mut connection)
            .await
            .map_err(|err| AirbenderProcessorError::GeneralError(err.to_string()))?;

        // In the state keeper, this value is used to reject execution.
        // All batches have already been executed by State Keeper.
        // This means we don't want to reject any execution, therefore we're using MAX as an allow all.
        let validation_computational_gas_limit = u32::MAX;

        let RestoredL1BatchEnv {
            system_env,
            l1_batch_env,
            pubdata_params,
            ..
        } = l1_batch_params_provider
            .load_l1_batch_env(
                &mut connection,
                l1_batch_number,
                validation_computational_gas_limit,
                self.l2_chain_id,
            )
            .await
            .map_err(|err| AirbenderProcessorError::GeneralError(err.to_string()))?
            .ok_or(AirbenderProcessorError::GeneralError(
                "system_env, l1_batch_env missing".into(),
            ))?;

        Ok(AirbenderVerifierInput::new(V1AirbenderVerifierInput {
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
        min_batch_number: L1BatchNumber,
    ) -> Result<Option<LockedBatch>, AirbenderProcessorError> {
        self.pool
            .connection_tagged("airbender_request_processor")
            .await?
            .airbender_proof_generation_dal()
            .lock_batch_for_proving(self.config.proof_generation_timeout, min_batch_number)
            .await
            .map_err(Into::into)
    }

    async fn unlock_batch(
        &self,
        l1_batch_number: L1BatchNumber,
        status: AirbenderProofGenerationJobStatus,
    ) -> Result<(), AirbenderProcessorError> {
        self.pool
            .connection_tagged("airbender_request_processor")
            .await?
            .airbender_proof_generation_dal()
            .unlock_batch(l1_batch_number, status)
            .await?;
        Ok(())
    }

    pub(crate) async fn get_proof_generation_data_for_batch(
        &self,
        Path(l1_batch_number): Path<u32>,
    ) -> Result<String, AirbenderProcessorError> {
        let l1_batch_number = L1BatchNumber(l1_batch_number);
        tracing::info!("Received request for proof generation data for batch {l1_batch_number}");

        let input = self
            .airbender_verifier_input_for_existing_batch(l1_batch_number)
            .await?;
        let hex = encode_input_to_hex(&input).map_err(|err| {
            AirbenderProcessorError::GeneralError(format!(
                "Failed to encode verifier input for batch {l1_batch_number}: {err}"
            ))
        })?;
        Ok(hex)
    }

    pub(crate) async fn submit_proof(
        &self,
        Path(l1_batch_number): Path<u32>,
        Json(proof): Json<SubmitAirbenderProofRequest>,
    ) -> Result<Json<SubmitAirbenderProofResponse>, AirbenderProcessorError> {
        let l1_batch_number = L1BatchNumber(l1_batch_number);

        let proof_for_gcs = L1BatchAirbenderProofForL1 { proof: proof.proof };
        let proof_blob_url = self
            .blob_store
            .put(l1_batch_number, &proof_for_gcs)
            .await
            .map_err(|source| AirbenderProcessorError::ObjectStore {
                source,
                context: "Failed to upload proof to GCS".into(),
            })?;

        let mut connection = self
            .pool
            .connection_tagged("airbender_request_processor")
            .await?;
        let mut dal = connection.airbender_proof_generation_dal();
        dal.save_proof_artifacts_metadata(l1_batch_number, &proof_blob_url)
            .await?;

        let sealed_at = connection
            .blocks_dal()
            .get_batch_sealed_at(l1_batch_number)
            .await?;

        let duration = sealed_at.and_then(|sealed_at| (Utc::now() - sealed_at).to_std().ok());

        let duration_secs_f64 = if let Some(duration) = duration {
            METRICS.airbender_proof_roundtrip_time.observe(duration);
            duration.as_secs_f64()
        } else {
            f64::NAN
        };

        tracing::info!(
            l1_batch_number = %l1_batch_number,
            sealed_to_proven_in_secs = duration_secs_f64,
            "Received proof for batch {}",
            l1_batch_number
        );

        Ok(Json(SubmitAirbenderProofResponse::Success))
    }
}
