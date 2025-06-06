use std::sync::Arc;

use axum::{extract::Path, Json};
use chrono::{Duration as ChronoDuration, Utc};
use zksync_config::configs::TeeProofDataHandlerConfig;
use zksync_crypto_primitives::Signature;
use zksync_dal::{
    tee_proof_generation_dal::{LockedBatch, TeeProofGenerationJobStatus},
    ConnectionPool, Core, CoreDal,
};
use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_prover_interface::{
    inputs::{VMRunWitnessInputData, WitnessInputMerklePaths},
    Bincode,
};
use zksync_tee_prover_interface::{
    api::{
        RegisterTeeAttestationRequest, RegisterTeeAttestationResponse, SubmitTeeProofRequest,
        SubmitTeeProofResponse, TeeProofGenerationDataRequest, TeeProofGenerationDataResponse,
    },
    inputs::{TeeVerifierInput, V1TeeVerifierInput},
};
use zksync_types::{
    public_to_address, recover, tee_types::TeeType, L1BatchNumber, L2ChainId, H256,
};
use zksync_vm_executor::storage::L1BatchParamsProvider;

use crate::{
    collateral::update_collateral_for_quote,
    errors::{TeeProcessorContext, TeeProcessorError},
    metrics::METRICS,
    tee_contract::TeeFunctions,
};

#[derive(Clone)]
pub(crate) struct TeeRequestProcessor {
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Core>,
    config: TeeProofDataHandlerConfig,
    l2_chain_id: L2ChainId,
    functions: TeeFunctions,
}

impl TeeRequestProcessor {
    pub(crate) fn new(
        blob_store: Arc<dyn ObjectStore>,
        pool: ConnectionPool<Core>,
        config: TeeProofDataHandlerConfig,
        l2_chain_id: L2ChainId,
    ) -> Self {
        Self {
            blob_store,
            pool,
            config,
            l2_chain_id,
            functions: TeeFunctions::default(),
        }
    }

    pub(crate) async fn get_proof_generation_data(
        &self,
        request: Json<TeeProofGenerationDataRequest>,
    ) -> Result<Option<Json<TeeProofGenerationDataResponse>>, TeeProcessorError> {
        tracing::info!("Received request for proof generation data: {:?}", request);

        let batch_ignored_timeout =
            ChronoDuration::from_std(self.config.batch_permanently_ignored_timeout_in_hours)
                .context("Failed to convert batch_ignored_timeout")?;
        let min_batch_number = self.config.first_processed_batch;

        loop {
            let Some(locked_batch) = self
                .lock_batch_for_proving(request.tee_type, min_batch_number)
                .await?
            else {
                break Ok(None); // no job available
            };
            let batch_number = locked_batch.l1_batch_number;

            match self
                .tee_verifier_input_for_existing_batch(batch_number)
                .await
            {
                Ok(input) => {
                    break Ok(Some(Json(TeeProofGenerationDataResponse(Box::new(input)))));
                }
                Err(TeeProcessorError::ObjectStore {
                    source: ObjectStoreError::KeyNotFound(_),
                    context,
                }) => {
                    let duration = Utc::now().signed_duration_since(locked_batch.created_at);
                    let status = if duration > batch_ignored_timeout {
                        TeeProofGenerationJobStatus::PermanentlyIgnored
                    } else {
                        TeeProofGenerationJobStatus::Failed
                    };
                    self.unlock_batch(batch_number, request.tee_type, status)
                        .await?;
                    tracing::warn!(
                        "Assigned status `{}` to batch {} created at {}: {context}",
                        status,
                        batch_number,
                        locked_batch.created_at
                    );
                }
                Err(err) => {
                    self.unlock_batch(
                        batch_number,
                        request.tee_type,
                        TeeProofGenerationJobStatus::Failed,
                    )
                    .await?;
                    break Err(err);
                }
            }
        }
    }

    #[tracing::instrument(skip(self))]
    async fn tee_verifier_input_for_existing_batch(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<TeeVerifierInput, TeeProcessorError> {
        let vm_run_data: VMRunWitnessInputData = match self.blob_store.get(l1_batch_number).await {
            Ok(data) => data,
            Err(_) => self
                .blob_store
                .get::<VMRunWitnessInputData<Bincode>>(l1_batch_number)
                .await
                .map(Into::into)
                .map_err(|source| TeeProcessorError::ObjectStore {
                    source,
                    context: "Failed to get VMRunWitnessInputData".into(),
                })?,
        };

        let merkle_paths: WitnessInputMerklePaths = match self.blob_store.get(l1_batch_number).await
        {
            Ok(data) => data,
            Err(_) => self
                .blob_store
                .get::<WitnessInputMerklePaths<Bincode>>(l1_batch_number)
                .await
                .map(Into::into)
                .map_err(|source| TeeProcessorError::ObjectStore {
                    source,
                    context: "Failed to get WitnessInputMerklePaths".into(),
                })?,
        };

        let mut connection = self.pool.connection_tagged("tee_request_processor").await?;

        let l2_blocks_execution_data = connection
            .transactions_dal()
            .get_l2_blocks_to_execute_for_l1_batch(l1_batch_number)
            .await?;

        let l1_batch_params_provider = L1BatchParamsProvider::new(&mut connection)
            .await
            .map_err(|err| TeeProcessorError::GeneralError(err.to_string()))?;

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
            .map_err(|err| TeeProcessorError::GeneralError(err.to_string()))?
            .ok_or(TeeProcessorError::GeneralError(
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
    ) -> Result<Option<LockedBatch>, TeeProcessorError> {
        self.pool
            .connection_tagged("tee_request_processor")
            .await?
            .tee_proof_generation_dal()
            .lock_batch_for_proving(
                tee_type,
                self.config.proof_generation_timeout_in_secs,
                min_batch_number,
            )
            .await
            .map_err(Into::into)
    }

    async fn unlock_batch(
        &self,
        l1_batch_number: L1BatchNumber,
        tee_type: TeeType,
        status: TeeProofGenerationJobStatus,
    ) -> Result<(), TeeProcessorError> {
        self.pool
            .connection_tagged("tee_request_processor")
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
    ) -> Result<Json<SubmitTeeProofResponse>, TeeProcessorError> {
        let l1_batch_number = L1BatchNumber(l1_batch_number);
        let mut connection = self.pool.connection_tagged("tee_request_processor").await?;
        let root_hash = connection
            .blocks_dal()
            .get_l1_batch_state_root(l1_batch_number)
            .await?
            .ok_or(TeeProcessorError::GeneralError(
                "L1 Batch has no root hash".into(),
            ))?;

        // Do some sanity checks before saving the proof
        if root_hash.as_bytes() != proof.0.proof {
            return Err(TeeProcessorError::GeneralError(
                "root hash does not match".into(),
            ));
        }

        // Check for a valid signature
        let root_hash = H256::from_slice(proof.0.proof.as_slice()).into();
        let signature = Signature::from_electrum(&proof.0.signature);
        let recovered_pubkey = recover(&signature, &root_hash).unwrap();

        /*
                let proof_address = public_to_address(&recovered_pubkey);

                // FIXME: TEE - pubkey must be converted somehow
                if recovered_pubkey.as_bytes() != proof.0.pubkey {
                    tracing::warn!(
                        "Invalid signature proof_address {} != pubkey {}",
                        hex::encode(recovered_pubkey.as_bytes()),
                        hex::encode(proof.0.pubkey)
                    );

                    return Err(TeeProcessorError::GeneralError("invalid signature".into()));
                }
        */

        let calldata = self
            .functions
            .verify_digest(root_hash, proof.0.signature.clone())
            .unwrap();

        let mut dal = connection.tee_proof_generation_dal();
        dal.save_proof_artifacts_metadata(
            l1_batch_number,
            proof.0.tee_type,
            &proof.0.pubkey,
            &proof.0.signature,
            &proof.0.proof,
            &calldata,
        )
        .await?;

        let sealed_at = connection
            .blocks_dal()
            .get_batch_sealed_at(l1_batch_number)
            .await?;

        let duration = sealed_at.and_then(|sealed_at| (Utc::now() - sealed_at).to_std().ok());

        let duration_secs_f64 = if let Some(duration) = duration {
            METRICS.tee_proof_roundtrip_time[&proof.0.tee_type.into()].observe(duration);
            duration.as_secs_f64()
        } else {
            f64::NAN
        };

        tracing::info!(
            l1_batch_number = %l1_batch_number,
            sealed_to_proven_in_secs = duration_secs_f64,
            "Received proof {:?}",
            proof
        );

        Ok(Json(SubmitTeeProofResponse::Success))
    }

    pub(crate) async fn register_tee_attestation(
        &self,
        Json(payload): Json<RegisterTeeAttestationRequest>,
    ) -> Result<Json<RegisterTeeAttestationResponse>, TeeProcessorError> {
        tracing::info!("Received attestation: {:?}", payload);

        let mut connection = self.pool.connection_tagged("tee_request_processor").await?;

        update_collateral_for_quote(&mut connection, &payload.attestation, &self.functions).await?;

        let mut dal = connection.tee_proof_generation_dal();
        let calldata = self
            .functions
            .register_signer(payload.attestation.clone())
            .unwrap();

        dal.save_attestation(&payload.pubkey, &payload.attestation, &calldata)
            .await?;

        Ok(Json(RegisterTeeAttestationResponse::Success))
    }
}
