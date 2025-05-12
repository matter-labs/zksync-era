use std::{collections::HashMap, time::Instant};

use async_trait::async_trait;
use circuit_definitions::{
    circuit_definitions::recursion_layer::{ZkSyncRecursionLayerStorageType, ZkSyncRecursionProof},
    zkevm_circuits::scheduler::aux::BaseLayerCircuitType,
};
use zkevm_test_harness::empty_node_proof;
use zksync_circuit_prover_service::types::circuit_wrapper::CircuitWrapper;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::{keys::FriCircuitKey, FriProofWrapper};
use zksync_types::basic_fri_types::AggregationRound;

use crate::{
    artifacts::{ArtifactsManager, JobId},
    rounds::recursion_tip::{RecursionTip, RecursionTipArtifacts},
};

#[async_trait]
impl ArtifactsManager for RecursionTip {
    type InputMetadata = Vec<(u8, JobId)>;
    type InputArtifacts = Vec<ZkSyncRecursionProof>;
    type OutputArtifacts = RecursionTipArtifacts;
    type BlobUrls = String;

    /// Loads all proofs for a given recursion tip's job ids.
    /// Note that recursion tip may not have proofs for some specific circuits (because the batch didn't contain them).
    /// In this scenario, we still need to pass a proof, but it won't be taken into account during proving.
    /// For this scenario, we use an empty_proof, but any proof would suffice.
    async fn get_artifacts(
        metadata: &Vec<(u8, JobId)>,
        object_store: &dyn ObjectStore,
    ) -> anyhow::Result<Vec<ZkSyncRecursionProof>> {
        let job_mapping: HashMap<u8, JobId> = metadata
            .clone()
            .into_iter()
            .map(|(leaf_circuit_id, job_id)| {
                (
                    ZkSyncRecursionLayerStorageType::from_leaf_u8_to_basic_u8(leaf_circuit_id),
                    job_id,
                )
            })
            .collect();

        let empty_proof = empty_node_proof().into_inner();

        let mut proofs = Vec::new();
        for circuit_id in BaseLayerCircuitType::as_iter_u8() {
            if job_mapping.contains_key(&circuit_id) {
                let key = *job_mapping.get(&circuit_id).unwrap();
                let fri_proof_wrapper = object_store
                    .get((key.id(), key.chain_id()))
                    .await
                    .unwrap_or_else(|_| {
                        panic!(
                            "Failed to load proof with circuit_id {} for recursion tip",
                            circuit_id
                        )
                    });
                match fri_proof_wrapper {
                    FriProofWrapper::Base(_) => {
                        return Err(anyhow::anyhow!(
                        "Expected only recursive proofs for recursion tip, got Base for circuit {}",
                        circuit_id
                    ));
                    }
                    FriProofWrapper::Recursive(recursive_proof) => {
                        proofs.push(recursive_proof.into_inner());
                    }
                }
            } else {
                proofs.push(empty_proof.clone());
            }
        }
        Ok(proofs)
    }

    async fn save_to_bucket(
        job_id: JobId,
        artifacts: Self::OutputArtifacts,
        object_store: &dyn ObjectStore,
    ) -> String {
        let key = FriCircuitKey {
            batch_id: job_id.into(),
            circuit_id: 255,
            sequence_number: 0,
            depth: 0,
            aggregation_round: AggregationRound::RecursionTip,
        };

        object_store
            .put(
                key,
                &CircuitWrapper::Recursive(artifacts.recursion_tip_circuit.clone()),
            )
            .await
            .unwrap()
    }

    async fn save_to_database(
        connection_pool: &ConnectionPool<Prover>,
        job_id: JobId,
        started_at: Instant,
        blob_urls: String,
        _artifacts: Self::OutputArtifacts,
    ) -> anyhow::Result<()> {
        let mut prover_connection = connection_pool.connection().await?;
        let mut transaction = prover_connection.start_transaction().await?;
        let protocol_version_id = transaction
            .fri_basic_witness_generator_dal()
            .protocol_version_for_l1_batch(job_id.into())
            .await
            .unwrap();
        let batch_sealed_at = transaction
            .fri_basic_witness_generator_dal()
            .get_batch_sealed_at_timestamp(job_id.into())
            .await;

        transaction
            .fri_prover_jobs_dal()
            .insert_prover_job(
                job_id.into(),
                ZkSyncRecursionLayerStorageType::RecursionTipCircuit as u8,
                0,
                0,
                AggregationRound::RecursionTip,
                &blob_urls,
                false,
                protocol_version_id,
                batch_sealed_at,
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to insert prover job: {}", e))?;

        transaction
            .fri_recursion_tip_witness_generator_dal()
            .mark_recursion_tip_job_as_successful(job_id.into(), started_at.elapsed())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to mark recursion tip job as successful: {}", e))?;

        transaction.commit().await?;

        Ok(())
    }
}
