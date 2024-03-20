#![cfg_attr(not(feature = "gpu"), allow(unused_imports))]

use std::{sync::Arc, time::Instant};

use prover_dal::{Connection, Prover, ProverDal};
use tokio::sync::Mutex;
use zkevm_test_harness::prover_utils::{
    verify_base_layer_proof, verify_eip4844_proof, verify_recursion_layer_proof,
};
use zksync_object_store::ObjectStore;
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::{
            algebraic_props::{
                round_function::AbsorptionModeOverwrite, sponge::GoldilocksPoseidon2Sponge,
            },
            cs::implementations::{pow::NoPow, proof::Proof, verifier::VerificationKey},
            field::goldilocks::{GoldilocksExt2, GoldilocksField},
        },
        circuit_definitions::recursion_layer::{
            ZkSyncRecursionLayerProof, ZkSyncRecursionLayerStorageType,
        },
    },
    queue::FixedSizeQueue,
    CircuitWrapper, FriProofWrapper, ProverServiceDataKey, WitnessVectorArtifacts,
    EIP_4844_CIRCUIT_ID,
};
use zksync_prover_fri_utils::get_base_layer_circuit_id_for_recursive_layer;
use zksync_types::{
    basic_fri_types::{AggregationRound, CircuitIdRoundTuple},
    L1BatchNumber,
};

use crate::metrics::METRICS;

pub type F = GoldilocksField;
pub type H = GoldilocksPoseidon2Sponge<AbsorptionModeOverwrite>;
pub type Ext = GoldilocksExt2;

#[cfg(feature = "gpu")]
pub type SharedWitnessVectorQueue = Arc<Mutex<FixedSizeQueue<GpuProverJob>>>;

pub struct ProverArtifacts {
    block_number: L1BatchNumber,
    pub proof_wrapper: FriProofWrapper,
}

impl ProverArtifacts {
    pub fn new(block_number: L1BatchNumber, proof_wrapper: FriProofWrapper) -> Self {
        Self {
            block_number,
            proof_wrapper,
        }
    }
}

#[cfg(feature = "gpu")]
pub struct GpuProverJob {
    pub witness_vector_artifacts: WitnessVectorArtifacts,
}

pub async fn save_proof(
    job_id: u32,
    started_at: Instant,
    artifacts: ProverArtifacts,
    blob_store: &dyn ObjectStore,
    public_blob_store: Option<&dyn ObjectStore>,
    shall_save_to_public_bucket: bool,
    storage_processor: &mut Connection<'_, Prover>,
) {
    tracing::info!(
        "Successfully proven job: {}, total time taken: {:?}",
        job_id,
        started_at.elapsed()
    );
    let proof = artifacts.proof_wrapper;

    // We save the scheduler proofs in public bucket,
    // so that it can be verified independently while we're doing shadow proving
    let (circuit_type, is_scheduler_proof) = match &proof {
        FriProofWrapper::Base(base) => (base.numeric_circuit_type(), false),
        FriProofWrapper::Recursive(recursive_circuit) => match recursive_circuit {
            ZkSyncRecursionLayerProof::SchedulerCircuit(_) => {
                if shall_save_to_public_bucket {
                    public_blob_store
                        .expect("public_object_store shall not be empty while running with shall_save_to_public_bucket config")
                        .put(artifacts.block_number.0, &proof)
                        .await
                        .unwrap();
                }
                (recursive_circuit.numeric_circuit_type(), true)
            }
            _ => (recursive_circuit.numeric_circuit_type(), false),
        },
        FriProofWrapper::Eip4844(_) => (ProverServiceDataKey::eip4844().circuit_id, false),
    };

    let blob_save_started_at = Instant::now();
    let blob_url = blob_store.put(job_id, &proof).await.unwrap();

    METRICS.blob_save_time[&circuit_type.to_string()].observe(blob_save_started_at.elapsed());

    let mut transaction = storage_processor.start_transaction().await.unwrap();
    let job_metadata = transaction
        .fri_prover_jobs_dal()
        .save_proof(job_id, started_at.elapsed(), &blob_url)
        .await;
    if is_scheduler_proof {
        transaction
            .fri_proof_compressor_dal()
            .insert_proof_compression_job(artifacts.block_number, &blob_url)
            .await;
    }
    if job_metadata.is_node_final_proof {
        let circuit_id = if job_metadata.circuit_id == EIP_4844_CIRCUIT_ID {
            EIP_4844_CIRCUIT_ID
        } else {
            get_base_layer_circuit_id_for_recursive_layer(job_metadata.circuit_id)
        };
        transaction
            .fri_scheduler_dependency_tracker_dal()
            .set_final_prover_job_id_for_l1_batch(
                circuit_id,
                job_id,
                job_metadata.block_number,
                job_metadata.sequence_number,
            )
            .await;
    }
    transaction.commit().await.unwrap();
}

pub fn verify_proof(
    circuit_wrapper: &CircuitWrapper,
    proof: &Proof<F, H, Ext>,
    vk: &VerificationKey<F, H>,
    job_id: u32,
) {
    let started_at = Instant::now();
    let (is_valid, circuit_id) = match circuit_wrapper {
        CircuitWrapper::Base(base_circuit) => (
            verify_base_layer_proof::<NoPow>(base_circuit, proof, vk),
            base_circuit.numeric_circuit_type(),
        ),
        CircuitWrapper::Recursive(recursive_circuit) => (
            verify_recursion_layer_proof::<NoPow>(recursive_circuit, proof, vk),
            recursive_circuit.numeric_circuit_type(),
        ),
        CircuitWrapper::Eip4844(_) => (
            verify_eip4844_proof::<NoPow>(proof, vk),
            ProverServiceDataKey::eip4844().circuit_id,
        ),
    };

    METRICS.proof_verification_time[&circuit_id.to_string()].observe(started_at.elapsed());

    if !is_valid {
        let msg = format!("Failed to verify proof for job-id: {job_id} circuit_type {circuit_id}");
        tracing::error!("{}", msg);
        panic!("{}", msg);
    }
}

pub fn setup_metadata_to_setup_data_key(
    setup_metadata: &CircuitIdRoundTuple,
) -> ProverServiceDataKey {
    ProverServiceDataKey {
        circuit_id: setup_metadata.circuit_id,
        round: setup_metadata.aggregation_round.into(),
    }
}

pub fn get_setup_data_key(key: ProverServiceDataKey) -> ProverServiceDataKey {
    match key.round {
        AggregationRound::NodeAggregation => {
            // For node aggregation only one key exist for all circuit types
            ProverServiceDataKey {
                circuit_id: ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
                round: key.round,
            }
        }
        _ => key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_setup_data_key_for_node_agg_key() {
        let key = ProverServiceDataKey {
            circuit_id: 10,
            round: AggregationRound::NodeAggregation,
        };
        let expected = ProverServiceDataKey {
            circuit_id: ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
            round: AggregationRound::NodeAggregation,
        };

        let result = get_setup_data_key(key);

        // Check if the `circuit_id` has been changed to `NodeLayerCircuit's` id
        assert_eq!(expected, result);
    }

    #[test]
    fn test_get_setup_data_key_for_non_node_agg_key() {
        let key = ProverServiceDataKey {
            circuit_id: 10,
            round: AggregationRound::BasicCircuits,
        };

        let result = get_setup_data_key(key.clone());

        // Check if the key has remained same
        assert_eq!(key, result);
    }
}
