use std::collections::HashMap;
use std::time::Instant;

use zksync_dal::ConnectionPool;
use zksync_object_store::gcs_utils::{
    final_node_aggregations_blob_url, scheduler_witness_blob_url,
};
use zksync_object_store::object_store::{DynamicObjectStore, SCHEDULER_WITNESS_JOBS_BUCKET_PATH};
use zksync_types::{
    circuit::{
        LEAF_CIRCUIT_INDEX, LEAF_SPLITTING_FACTOR, NODE_CIRCUIT_INDEX, NODE_SPLITTING_FACTOR,
    },
    proofs::{
        AggregationRound, PrepareSchedulerCircuitJob, WitnessGeneratorJob,
        WitnessGeneratorJobInput, WitnessGeneratorJobMetadata,
    },
    zkevm_test_harness::{
        abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit,
        bellman::{bn256::Bn256, plonk::better_better_cs::setup::VerificationKey},
        sync_vm::scheduler::BlockApplicationWitness,
        witness::{self, oracle::VmWitnessOracle, recursive_aggregation::erase_vk_type},
        NodeAggregationOutputDataWitness, SchedulerCircuitInstanceWitness,
    },
    L1BatchNumber,
};
use zksync_verification_key_server::{
    get_vk_for_circuit_type, get_vks_for_basic_circuits, get_vks_for_commitment,
};

use crate::witness_generator;
use crate::witness_generator::track_witness_generation_stage;
use crate::witness_generator::utils::save_prover_input_artifacts;

pub struct SchedulerArtifacts {
    pub final_aggregation_result: BlockApplicationWitness<Bn256>,
    pub serialized_circuits: Vec<(String, Vec<u8>)>,
}

pub fn process_scheduler_job(
    started_at: Instant,
    block_number: L1BatchNumber,
    job: PrepareSchedulerCircuitJob,
) -> SchedulerArtifacts {
    let stage_started_at = Instant::now();

    let verification_keys: HashMap<
        u8,
        VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
    > = get_vks_for_basic_circuits();

    let (_, set_committment, g2_points) =
        witness::recursive_aggregation::form_base_circuits_committment(get_vks_for_commitment(
            verification_keys,
        ));

    vlog::info!(
        "Verification keys loaded in {:?}",
        stage_started_at.elapsed()
    );

    let leaf_aggregation_vk = get_vk_for_circuit_type(LEAF_CIRCUIT_INDEX);

    let node_aggregation_vk = get_vk_for_circuit_type(NODE_CIRCUIT_INDEX);

    let (_, leaf_aggregation_vk_committment) =
        witness::recursive_aggregation::compute_vk_encoding_and_committment(erase_vk_type(
            leaf_aggregation_vk,
        ));

    let (_, node_aggregation_vk_committment) =
        witness::recursive_aggregation::compute_vk_encoding_and_committment(erase_vk_type(
            node_aggregation_vk.clone(),
        ));

    vlog::info!("Commitments generated in {:?}", stage_started_at.elapsed());
    let stage_started_at = Instant::now();

    // fs::write("incomplete_scheduler_witness.bincode", bincode::serialize(&job.incomplete_scheduler_witness).unwrap()).unwrap();
    // fs::write("node_final_proof_level_proofs.bincode", bincode::serialize(&job.node_final_proof_level_proof).unwrap()).unwrap();
    // fs::write("node_aggregation_vk.bincode", bincode::serialize(&node_aggregation_vk).unwrap()).unwrap();
    // fs::write("final_node_aggregations.bincode", bincode::serialize(&job.final_node_aggregations).unwrap()).unwrap();
    // fs::write("leaf_vks_committment.bincode", bincode::serialize(&set_committment).unwrap()).unwrap();
    // fs::write("node_aggregation_vk_committment.bincode", bincode::serialize(&node_aggregation_vk_committment).unwrap()).unwrap();
    // fs::write("leaf_aggregation_vk_committment.bincode", bincode::serialize(&leaf_aggregation_vk_committment).unwrap()).unwrap();
    // fs::write("previous_aux_hash.bincode", bincode::serialize(&job.previous_aux_hash).unwrap()).unwrap();
    // fs::write("previous_meta_hash.bincode", bincode::serialize(&job.previous_meta_hash).unwrap()).unwrap();
    // fs::write("g2_points.bincode", bincode::serialize(&g2_points).unwrap()).unwrap();

    let (scheduler_circuit, final_aggregation_result) =
        zksync_types::zkevm_test_harness::witness::recursive_aggregation::prepare_scheduler_circuit(
            job.incomplete_scheduler_witness,
            job.node_final_proof_level_proof,
            node_aggregation_vk,
            job.final_node_aggregations,
            set_committment,
            node_aggregation_vk_committment,
            leaf_aggregation_vk_committment,
            job.previous_aux_hash,
            job.previous_meta_hash,
            (LEAF_SPLITTING_FACTOR * NODE_SPLITTING_FACTOR) as u32,
            g2_points,
        );

    vlog::info!(
        "prepare_scheduler_circuit took {:?}",
        stage_started_at.elapsed()
    );

    let serialized_circuits: Vec<(String, Vec<u8>)> =
        witness_generator::serialize_circuits(&vec![scheduler_circuit]);

    vlog::info!(
        "Scheduler generation for block {} is complete in {:?}",
        block_number.0,
        started_at.elapsed()
    );

    SchedulerArtifacts {
        final_aggregation_result,
        serialized_circuits,
    }
}

pub fn update_database(
    connection_pool: ConnectionPool,
    started_at: Instant,
    block_number: L1BatchNumber,
    final_aggregation_result: BlockApplicationWitness<Bn256>,
    circuits: Vec<String>,
) {
    let mut connection = connection_pool.access_storage_blocking();
    let mut transaction = connection.start_transaction_blocking();
    let block = transaction
        .blocks_dal()
        .get_block_metadata(block_number)
        .expect("L1 batch should exist");

    assert_eq!(
        block.metadata.aux_data_hash.0, final_aggregation_result.aux_data_hash,
        "Commitment for aux data is wrong"
    );

    assert_eq!(
        block.metadata.pass_through_data_hash.0, final_aggregation_result.passthrough_data_hash,
        "Commitment for pass through data is wrong"
    );

    assert_eq!(
        block.metadata.meta_parameters_hash.0, final_aggregation_result.meta_data_hash,
        "Commitment for metadata is wrong"
    );

    assert_eq!(
        block.metadata.commitment.0, final_aggregation_result.block_header_hash,
        "Commitment is wrong"
    );

    transaction.prover_dal().insert_prover_jobs(
        block_number,
        circuits,
        AggregationRound::Scheduler,
    );

    transaction
        .witness_generator_dal()
        .save_final_aggregation_result(
            block_number,
            final_aggregation_result.aggregation_result_coords,
        );

    transaction
        .witness_generator_dal()
        .mark_witness_job_as_successful(
            block_number,
            AggregationRound::Scheduler,
            started_at.elapsed(),
        );

    transaction.commit_blocking();
    track_witness_generation_stage(started_at, AggregationRound::Scheduler);
}

pub async fn save_artifacts(
    block_number: L1BatchNumber,
    serialized_circuits: Vec<(String, Vec<u8>)>,
    object_store: &mut DynamicObjectStore,
) {
    save_prover_input_artifacts(
        block_number,
        serialized_circuits,
        object_store,
        AggregationRound::Scheduler,
    )
    .await;
}

pub async fn get_artifacts(
    metadata: WitnessGeneratorJobMetadata,
    previous_aux_hash: [u8; 32],
    previous_meta_hash: [u8; 32],
    object_store: &DynamicObjectStore,
) -> WitnessGeneratorJob {
    let scheduler_witness_serialized = object_store
        .get(
            SCHEDULER_WITNESS_JOBS_BUCKET_PATH,
            scheduler_witness_blob_url(metadata.block_number),
        )
        .unwrap();
    let scheduler_witness = bincode::deserialize::<SchedulerCircuitInstanceWitness<Bn256>>(
        &scheduler_witness_serialized,
    )
    .expect("scheduler_witness deserialization failed");

    let final_node_aggregations_serialized = object_store
        .get(
            SCHEDULER_WITNESS_JOBS_BUCKET_PATH,
            final_node_aggregations_blob_url(metadata.block_number),
        )
        .expect("final_node_aggregations is not found in a `queued` `scheduler_witness_jobs` job");
    let final_node_aggregations = bincode::deserialize::<NodeAggregationOutputDataWitness<Bn256>>(
        &final_node_aggregations_serialized,
    )
    .expect("final_node_aggregations deserialization failed");

    WitnessGeneratorJob {
        block_number: metadata.block_number,
        job: WitnessGeneratorJobInput::Scheduler(Box::new(PrepareSchedulerCircuitJob {
            incomplete_scheduler_witness: scheduler_witness,
            final_node_aggregations,
            node_final_proof_level_proof: metadata.proofs.into_iter().next().unwrap(),
            previous_aux_hash,
            previous_meta_hash,
        })),
    }
}
