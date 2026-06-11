use std::sync::Arc;

use anyhow::Context;
use axum::{extract::Path, Json};
use chrono::Utc;
use zksync_airbender_prover_interface::{
    api::{
        AirbenderPresentBatchesResponse, AirbenderSnarkInputsResponse, SubmitAirbenderProofRequest,
        SubmitAirbenderProofResponse, SubmitAirbenderSnarkProofRequest,
        SubmitAirbenderSnarkProofResponse,
    },
    inputs::{AirbenderVerifierInput, BlobHash, CommitmentInput},
    outputs::{L1BatchAirbenderProofForL1, L1BatchAirbenderSnarkProofForL1},
};
use zksync_config::configs::AirbenderProofDataHandlerConfig;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_l1_contract_interface::i_executor::commit::kzg::{
    pubdata_to_blob_commitments, pubdata_to_blob_linear_hashes, pubdata_to_blob_versioned_hashes,
};
use zksync_object_store::{ObjectStore, ObjectStoreError, StoredObject};
use zksync_prover_interface::{
    inputs::{VMRunWitnessInputData, WitnessInputMerklePaths},
    outputs::L1BatchProofForL1,
};
use zksync_types::{
    blob::num_blobs_required, commitment::L1BatchCommitmentMode, L1BatchNumber, L2ChainId,
};
use zksync_vm_executor::storage::{L1BatchParamsProvider, RestoredL1BatchEnv};

use crate::{
    errors::AirbenderProcessorError,
    metrics::{ProcessorErrorKind, ProofStage, METRICS},
};

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
    ) -> Result<Option<AirbenderVerifierInput>, AirbenderProcessorError> {
        tracing::debug!("Received request for proof generation data");

        let min_batch_number = self.config.first_processed_batch;
        let max_attempts = self.config.max_attempts;

        for attempt in 0..max_attempts {
            // Whole flow runs in a single transaction so that any mid-flight
            // failure (DB, blob store, panic) rolls back the lock and leaves
            // the batch free for the next request.
            let mut connection = self
                .pool
                .connection_tagged("airbender_request_processor")
                .await?;
            let mut transaction = connection.start_transaction().await?;

            // Record the protocol version the batch is proved under at lock time, so `submit_proof`
            // and the SNARK step reuse the exact same version (and blob key) instead of recomputing
            // it. The version is the batch's own minor version with the latest known patch for that
            // minor (chosen inside the lock query), so a batch is proven under the protocol it
            // executed with — not the globally latest version.
            let Some(locked_batch) = transaction
                .airbender_proof_generation_dal()
                .lock_batch_for_proving(self.config.proof_generation_timeout, min_batch_number)
                .await?
            else {
                return Ok(None); // no job available
            };
            let batch_number = locked_batch.l1_batch_number;

            match self
                .airbender_verifier_input_for_existing_batch(batch_number, &mut transaction)
                .await
            {
                Ok(input) => {
                    let protocol_version = locked_batch.protocol_version;
                    transaction.commit().await?;
                    METRICS.airbender_jobs_picked[&(ProofStage::Fri, protocol_version.to_string())]
                        .inc();
                    return Ok(Some(input));
                }
                Err(AirbenderProcessorError::ObjectStore {
                    source: ObjectStoreError::KeyNotFound(_),
                    context,
                }) => {
                    // Dropping the tx rolls the lock back so the batch is retryable.
                    drop(transaction);
                    METRICS.airbender_processor_errors[&ProcessorErrorKind::ObjectStoreKeyNotFound]
                        .inc();
                    tracing::warn!(
                        "Data not available on GCS for batch {} created at {} (attempt {}/{}): {context}",
                        batch_number,
                        locked_batch.created_at,
                        attempt + 1,
                        max_attempts,
                    );
                    continue; // try the next batch
                }
                Err(err) => {
                    return Err(err);
                }
            }
        }

        METRICS.airbender_processor_errors[&ProcessorErrorKind::AttemptsExhausted].inc();
        tracing::warn!("Exhausted {max_attempts} attempts to find a batch with available GCS data");
        Ok(None)
    }

    pub(crate) async fn get_proof_generation_data_no_lock(
        &self,
        Path(l1_batch_number): Path<u32>,
    ) -> Result<Option<AirbenderVerifierInput>, AirbenderProcessorError> {
        let l1_batch_number = L1BatchNumber(l1_batch_number);

        if !self
            .is_batch_present_for_airbender_proof_inputs(l1_batch_number)
            .await?
        {
            return Ok(None);
        }

        let mut connection = self
            .pool
            .connection_tagged("airbender_request_processor")
            .await?;

        match self
            .airbender_verifier_input_for_existing_batch(l1_batch_number, &mut connection)
            .await
        {
            Ok(input) => Ok(Some(input)),
            Err(AirbenderProcessorError::ObjectStore {
                source: ObjectStoreError::KeyNotFound(_),
                ..
            }) => Ok(None),
            Err(err) => Err(err),
        }
    }

    pub(crate) async fn get_present_batches(
        &self,
    ) -> Result<Json<AirbenderPresentBatchesResponse>, AirbenderProcessorError> {
        let bounds = self
            .pool
            .connection_tagged("airbender_request_processor")
            .await?
            .proof_generation_dal()
            .get_present_batch_bounds_for_airbender_proof_inputs(self.config.first_processed_batch)
            .await?;

        let (oldest_batch, latest_batch) = match bounds {
            Some((oldest_batch, latest_batch)) => (Some(oldest_batch.0), Some(latest_batch.0)),
            None => (None, None),
        };

        Ok(Json(AirbenderPresentBatchesResponse {
            oldest_batch,
            latest_batch,
        }))
    }

    #[tracing::instrument(skip(self, connection))]
    async fn airbender_verifier_input_for_existing_batch(
        &self,
        l1_batch_number: L1BatchNumber,
        connection: &mut Connection<'_, Core>,
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

        let l2_blocks_execution_data = connection
            .transactions_dal()
            .get_l2_blocks_to_execute_for_l1_batch(l1_batch_number)
            .await?;

        let l1_batch_params_provider = L1BatchParamsProvider::new(connection)
            .await
            .map_err(AirbenderProcessorError::GeneralError)?;

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
                connection,
                l1_batch_number,
                validation_computational_gas_limit,
                self.l2_chain_id,
            )
            .await
            .map_err(AirbenderProcessorError::GeneralError)?
            .ok_or_else(|| {
                AirbenderProcessorError::GeneralError(anyhow::anyhow!(
                    "system_env, l1_batch_env missing for batch {l1_batch_number}"
                ))
            })?;

        // Airbender V2 commitment chain assumes post-1.4.2 protocol semantics
        // (blob hashes computed from EIP-4844 pubdata).
        if system_env.version.is_pre_1_4_2() {
            return Err(AirbenderProcessorError::GeneralError(anyhow::anyhow!(
                "batch {l1_batch_number}: protocol version {:?} is pre-1.4.2 — \
                 Airbender V2 requires post-1.4.2 batches",
                system_env.version
            )));
        }

        // Prev-batch hashes from L1 settlement of batch N-1. For batch 1 we
        // read the genesis batch (#0) commitments, which
        // `genesis::insert_genesis_batch` populates at chain bootstrap — so
        // batch 1 is provable. Batch 0 itself has no predecessor and isn't a
        // valid proving target. The Airbender variant uses lighter
        // aux-commitment math (zero events queue + Blake2 bootloader heap), so
        // its `commitment` and `aux_data_hash` differ from Boojum's;
        // `meta_parameters_hash` matches and is read from the regular
        // `l1_batches` row in the same query.
        let prev_number = L1BatchNumber(l1_batch_number.0.checked_sub(1).ok_or_else(|| {
            AirbenderProcessorError::GeneralError(anyhow::anyhow!(
                "batch {l1_batch_number} has no predecessor (only batch 1+ are provable)"
            ))
        })?);
        let prev_commitment_input = connection
            .blocks_dal()
            .get_prev_batch_airbender_commitment_input(prev_number)
            .await?
            .ok_or_else(|| {
                AirbenderProcessorError::GeneralError(anyhow::anyhow!(
                    "previous batch {prev_number} has no Airbender commitment input yet — \
                     commitment_generator must run before Airbender V2 proving"
                ))
            })?;

        // Blob hashes + EIP-4844 versioned hashes from VM pubdata via KZG.
        let pubdata_input = connection
            .blocks_dal()
            .get_l1_batch_pubdata_input(l1_batch_number)
            .await?
            .ok_or_else(|| {
                AirbenderProcessorError::GeneralError(anyhow::anyhow!(
                    "pubdata_input missing for batch {l1_batch_number}"
                ))
            })?;
        let num_blobs = num_blobs_required(&system_env.version);

        let commitments = pubdata_to_blob_commitments(num_blobs, &pubdata_input);
        let versioned_hashes = pubdata_to_blob_versioned_hashes(num_blobs, &pubdata_input);
        let linear_hashes = pubdata_to_blob_linear_hashes(num_blobs, pubdata_input);

        // Mirror commitment_generator's Validium treatment: era zeros both
        // commitment and linear_hash in the persisted aux output for Validium
        // chains, so the prev-batch commitment chain that the verifier rebuilds
        // must use zeroed blob hashes too — otherwise the proof's commitment
        // disagrees with what era settled on L1.
        let commitment_mode: L1BatchCommitmentMode = pubdata_params.pubdata_type().into();
        let blob_hashes = match commitment_mode {
            L1BatchCommitmentMode::Rollup => commitments
                .into_iter()
                .zip(linear_hashes)
                .map(|(commitment, linear_hash)| BlobHash {
                    commitment,
                    linear_hash,
                })
                .collect::<Vec<_>>(),
            L1BatchCommitmentMode::Validium => vec![BlobHash::default(); num_blobs],
        };

        let commitment_input = CommitmentInput {
            prev_batch_commitment: prev_commitment_input.prev_batch_commitment,
            prev_meta_hash: prev_commitment_input.meta_parameters_hash,
            prev_aux_hash: prev_commitment_input.prev_aux_hash,
            blob_hashes,
            blob_versioned_hashes: versioned_hashes,
        };

        Ok(AirbenderVerifierInput {
            vm_run_data,
            merkle_paths,
            l2_blocks_execution_data,
            l1_batch_env,
            system_env,
            pubdata_params,
            commitment_input: Some(commitment_input),
        })
    }

    async fn is_batch_present_for_airbender_proof_inputs(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<bool, AirbenderProcessorError> {
        self.pool
            .connection_tagged("airbender_request_processor")
            .await?
            .proof_generation_dal()
            .is_batch_present_for_airbender_proof_inputs(
                l1_batch_number,
                self.config.first_processed_batch,
            )
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn submit_proof(
        &self,
        Json(proof): Json<SubmitAirbenderProofRequest>,
    ) -> Result<Json<SubmitAirbenderProofResponse>, AirbenderProcessorError> {
        let l1_batch_number = L1BatchNumber(proof.l1_batch_number);
        let prover_id = proof.prover_id;

        let mut connection = self
            .pool
            .connection_tagged("airbender_request_processor")
            .await?;

        // The version was recorded when the batch was locked for proving. If it's missing, the batch
        // was never picked, so reject the submission instead of guessing a key.
        let protocol_version = connection
            .airbender_proof_generation_dal()
            .get_batch_protocol_version(l1_batch_number)
            .await?
            .ok_or_else(|| {
                AirbenderProcessorError::GeneralError(anyhow::anyhow!(
                    "protocol version not recorded for batch {l1_batch_number}; was it picked for proving?"
                ))
            })?;

        let proof_for_gcs = L1BatchAirbenderProofForL1 { proof: proof.proof };
        let proof_blob_url = self
            .blob_store
            .put((l1_batch_number, protocol_version), &proof_for_gcs)
            .await
            .map_err(|source| AirbenderProcessorError::ObjectStore {
                source,
                context: "Failed to upload proof to GCS".into(),
            })?;

        let mut dal = connection.airbender_proof_generation_dal();
        dal.save_proof_artifacts_metadata(l1_batch_number, &proof_blob_url, &prover_id)
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

        METRICS.airbender_proofs_received[&(ProofStage::Fri, protocol_version.to_string())].inc();

        tracing::info!(
            l1_batch_number = %l1_batch_number,
            prover_id = %prover_id,
            sealed_to_proven_in_secs = duration_secs_f64,
            "Received proof for batch {}",
            l1_batch_number
        );

        Ok(Json(SubmitAirbenderProofResponse::Success))
    }

    pub(crate) async fn get_snark_inputs(
        &self,
    ) -> Result<Option<AirbenderSnarkInputsResponse>, AirbenderProcessorError> {
        tracing::debug!("Received request for SNARK inputs");

        let min_batch_number = self.config.first_processed_batch;
        let max_attempts = self.config.max_attempts;

        for attempt in 0..max_attempts {
            // Whole flow runs in a single transaction so that any mid-flight
            // failure (DB, blob store, panic) rolls back the lock and leaves
            // the batch free for the next request.
            let mut connection = self
                .pool
                .connection_tagged("airbender_request_processor")
                .await?;
            let mut transaction = connection.start_transaction().await?;

            let Some(locked_batch) = transaction
                .airbender_proof_generation_dal()
                .lock_batch_for_snark(self.config.snark_generation_timeout, min_batch_number)
                .await?
            else {
                return Ok(None);
            };
            let batch_number = locked_batch.l1_batch_number;
            let protocol_version = locked_batch.protocol_version;

            let proof: L1BatchAirbenderProofForL1 =
                match self.blob_store.get((batch_number, protocol_version)).await {
                    Ok(proof) => proof,
                    Err(ObjectStoreError::KeyNotFound(err)) => {
                        // Dropping the tx rolls the lock back to `generated`.
                        drop(transaction);
                        METRICS.airbender_processor_errors
                            [&ProcessorErrorKind::ObjectStoreKeyNotFound]
                            .inc();
                        tracing::warn!(
                            "FRI proof not available on GCS for batch {} (attempt {}/{}): {err}",
                            batch_number,
                            attempt + 1,
                            max_attempts,
                        );
                        continue;
                    }
                    Err(source) => {
                        return Err(AirbenderProcessorError::ObjectStore {
                            source,
                            context: "Failed to get L1BatchAirbenderProofForL1".into(),
                        });
                    }
                };

            transaction.commit().await?;

            METRICS.airbender_jobs_picked[&(ProofStage::Snark, protocol_version.to_string())].inc();

            return Ok(Some(AirbenderSnarkInputsResponse {
                l1_batch_number: batch_number.0,
                fri_proof: proof.proof,
            }));
        }

        METRICS.airbender_processor_errors[&ProcessorErrorKind::AttemptsExhausted].inc();
        tracing::warn!(
            "Exhausted {max_attempts} attempts to find a batch with available FRI proof"
        );
        Ok(None)
    }

    pub(crate) async fn submit_snark_proof(
        &self,
        Json(proof): Json<SubmitAirbenderSnarkProofRequest>,
    ) -> Result<Json<SubmitAirbenderSnarkProofResponse>, AirbenderProcessorError> {
        let l1_batch_number = L1BatchNumber(proof.l1_batch_number);
        let prover_id = proof.prover_id;

        let mut connection = self
            .pool
            .connection_tagged("airbender_request_processor")
            .await?;

        // The verifier submits the wrapper proof as a `SnarkWrapperProof`, which doesn't carry the
        // protocol version. Reuse the version recorded when the FRI proof was submitted so the SNARK
        // blob key matches and the L1 proof reports the correct version.
        let protocol_version = connection
            .airbender_proof_generation_dal()
            .get_batch_protocol_version(l1_batch_number)
            .await?
            .context("must exist")?;

        // Flatten the wrapper proof into the CBOR `L1BatchProofForL1` the eth_sender submits through
        // `proveBatches`, so the rest of the SNARK path mirrors Boojum proofs byte-for-byte.
        let l1_proof = L1BatchProofForL1::new_airbender_from_snark_wrapper(
            &proof.snark_proof,
            protocol_version,
        );
        let snark_proof =
            <L1BatchProofForL1 as StoredObject>::serialize(&l1_proof).map_err(|err| {
                AirbenderProcessorError::GeneralError(anyhow::anyhow!(
                    "Failed to CBOR-encode L1BatchProofForL1 for batch {l1_batch_number}: {err}"
                ))
            })?;

        let proof_for_gcs = L1BatchAirbenderSnarkProofForL1 { snark_proof };
        let snark_proof_blob_url = self
            .blob_store
            .put((l1_batch_number, protocol_version), &proof_for_gcs)
            .await
            .map_err(|source| AirbenderProcessorError::ObjectStore {
                source,
                context: "Failed to upload SNARK proof to GCS".into(),
            })?;

        connection
            .airbender_proof_generation_dal()
            .save_snark_proof_artifacts_metadata(l1_batch_number, &snark_proof_blob_url, &prover_id)
            .await?;

        let sealed_at = connection
            .blocks_dal()
            .get_batch_sealed_at(l1_batch_number)
            .await?;

        let duration = sealed_at.and_then(|sealed_at| (Utc::now() - sealed_at).to_std().ok());

        let duration_secs_f64 = if let Some(duration) = duration {
            METRICS.airbender_snark_roundtrip_time.observe(duration);
            duration.as_secs_f64()
        } else {
            f64::NAN
        };

        METRICS.airbender_proofs_received[&(ProofStage::Snark, protocol_version.to_string())].inc();

        tracing::info!(
            l1_batch_number = %l1_batch_number,
            prover_id = %prover_id,
            sealed_to_proven_in_secs = duration_secs_f64,
            "Received SNARK proof for batch {}",
            l1_batch_number
        );

        Ok(Json(SubmitAirbenderSnarkProofResponse::Success))
    }
}
