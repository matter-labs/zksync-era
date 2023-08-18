use std::sync::Arc;
use std::time::Instant;

use queues::{Buffer};
use tokio::sync::Mutex;
use zkevm_test_harness::prover_utils::{verify_base_layer_proof, verify_recursion_layer_proof};
use zksync_prover_fri_types::circuit_definitions::boojum::algebraic_props::round_function::AbsorptionModeOverwrite;
use zksync_prover_fri_types::circuit_definitions::boojum::algebraic_props::sponge::GoldilocksPoseidon2Sponge;
use zksync_prover_fri_types::circuit_definitions::boojum::cs::implementations::pow::NoPow;
use zksync_prover_fri_types::circuit_definitions::boojum::cs::implementations::proof::Proof;
use zksync_prover_fri_types::circuit_definitions::boojum::cs::implementations::verifier::VerificationKey;
use zksync_prover_fri_types::circuit_definitions::boojum::field::goldilocks::{
    GoldilocksExt2, GoldilocksField,
};
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerProof;

use zksync_config::configs::fri_prover_group::CircuitIdRoundTuple;
use zksync_dal::StorageProcessor;
use zksync_object_store::ObjectStore;
use zksync_prover_fri_types::{
    CircuitWrapper, FriProofWrapper, ProverServiceDataKey, WitnessVectorArtifacts,
};
use zksync_prover_fri_utils::get_base_layer_circuit_id_for_recursive_layer;

use zksync_types::L1BatchNumber;

pub type F = GoldilocksField;
pub type H = GoldilocksPoseidon2Sponge<AbsorptionModeOverwrite>;
pub type EXT = GoldilocksExt2;

pub type SharedWitnessVectorQueue = Arc<Mutex<Buffer<WitnessVectorArtifacts>>>;

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

pub async fn save_proof(
    job_id: u32,
    started_at: Instant,
    artifacts: ProverArtifacts,
    blob_store: &dyn ObjectStore,
    public_blob_store: &dyn ObjectStore,
    storage_processor: &mut StorageProcessor<'_>,
) {
    vlog::info!(
        "Successfully proven job: {}, took: {:?}",
        job_id,
        started_at.elapsed()
    );
    let proof = artifacts.proof_wrapper;

    // We save the scheduler proofs in public bucket,
    // so that it can be verified independently while we're doing shadow proving
    let circuit_type = match &proof {
        FriProofWrapper::Base(base) => base.numeric_circuit_type(),
        FriProofWrapper::Recursive(recursive_circuit) => match recursive_circuit {
            ZkSyncRecursionLayerProof::SchedulerCircuit(_) => {
                public_blob_store
                    .put(artifacts.block_number.0, &proof)
                    .await
                    .unwrap();
                recursive_circuit.numeric_circuit_type()
            }
            _ => recursive_circuit.numeric_circuit_type(),
        },
    };

    let blob_save_started_at = Instant::now();
    let blob_url = blob_store.put(job_id, &proof).await.unwrap();
    metrics::histogram!(
            "prover_fri.prover.blob_save_time",
            blob_save_started_at.elapsed(),
            "circuit_type" => circuit_type.to_string(),
    );

    let mut transaction = storage_processor.start_transaction().await;
    let job_metadata = transaction
        .fri_prover_jobs_dal()
        .save_proof(job_id, started_at.elapsed(), &blob_url)
        .await;
    if job_metadata.is_node_final_proof {
        transaction
            .fri_scheduler_dependency_tracker_dal()
            .set_final_prover_job_id_for_l1_batch(
                get_base_layer_circuit_id_for_recursive_layer(job_metadata.circuit_id),
                job_id,
                job_metadata.block_number,
            )
            .await;
    }
    transaction.commit().await;
}

pub fn verify_proof(
    circuit_wrapper: &CircuitWrapper,
    proof: &Proof<F, H, EXT>,
    vk: &VerificationKey<F, H>,
    job_id: u32,
) {
    let started_at = Instant::now();
    let (is_valid, circuit_id) = match circuit_wrapper {
        CircuitWrapper::Base(base_circuit) => (
            verify_base_layer_proof::<NoPow>(&base_circuit, proof, vk),
            base_circuit.numeric_circuit_type(),
        ),
        CircuitWrapper::Recursive(recursive_circuit) => (
            verify_recursion_layer_proof::<NoPow>(&recursive_circuit, proof, vk),
            recursive_circuit.numeric_circuit_type(),
        ),
    };
    metrics::histogram!(
        "prover_fri.prover.proof_verification_time",
        started_at.elapsed(),
        "circuit_type" => circuit_id.to_string(),
    );
    if !is_valid {
        vlog::error!(
            "Failed to verify base layer proof for job-id: {} circuit_type {}",
            job_id,
            circuit_id
        );
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
