use std::collections::HashMap;
use std::time::Instant;

use zksync_dal::ConnectionPool;
use zksync_object_store::gcs_utils::{
    aggregation_outputs_blob_url, basic_circuits_blob_url, basic_circuits_inputs_blob_url,
    leaf_layer_subqueues_blob_url,
};
use zksync_object_store::object_store::{
    DynamicObjectStore, LEAF_AGGREGATION_WITNESS_JOBS_BUCKET_PATH,
    NODE_AGGREGATION_WITNESS_JOBS_BUCKET_PATH,
};
use zksync_types::{
    circuit::LEAF_SPLITTING_FACTOR,
    proofs::{
        AggregationRound, PrepareLeafAggregationCircuitsJob, WitnessGeneratorJob,
        WitnessGeneratorJobInput, WitnessGeneratorJobMetadata,
    },
    zkevm_test_harness::{
        abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit,
        bellman::bn256::Bn256,
        bellman::plonk::better_better_cs::setup::VerificationKey,
        encodings::recursion_request::RecursionRequest,
        encodings::QueueSimulator,
        sync_vm, witness,
        witness::{
            full_block_artifact::{BlockBasicCircuits, BlockBasicCircuitsPublicInputs},
            oracle::VmWitnessOracle,
        },
    },
    L1BatchNumber,
};
use zksync_verification_key_server::{
    get_ordered_vks_for_basic_circuits, get_vks_for_basic_circuits, get_vks_for_commitment,
};

use crate::witness_generator;
use crate::witness_generator::track_witness_generation_stage;
use crate::witness_generator::utils::save_prover_input_artifacts;

pub struct LeafAggregationArtifacts {
    pub leaf_layer_subqueues: Vec<QueueSimulator<Bn256, RecursionRequest<Bn256>, 2, 2>>,
    pub aggregation_outputs:
        Vec<sync_vm::recursion::leaf_aggregation::LeafAggregationOutputDataWitness<Bn256>>,
    pub serialized_circuits: Vec<(String, Vec<u8>)>,
    pub leaf_circuits: Vec<ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
}

pub fn process_leaf_aggregation_job(
    started_at: Instant,
    block_number: L1BatchNumber,
    job: PrepareLeafAggregationCircuitsJob,
) -> LeafAggregationArtifacts {
    let stage_started_at = Instant::now();

    let verification_keys: HashMap<
        u8,
        VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
    > = get_vks_for_basic_circuits();

    vlog::info!(
        "Verification keys loaded in {:?}",
        stage_started_at.elapsed()
    );

    // we need the list of vks that matches the list of job.basic_circuit_proofs
    let vks_for_aggregation: Vec<
        VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
    > = get_ordered_vks_for_basic_circuits(&job.basic_circuits, &verification_keys);

    let (all_vk_committments, set_committment, g2_points) =
        witness::recursive_aggregation::form_base_circuits_committment(get_vks_for_commitment(
            verification_keys,
        ));

    vlog::info!("Commitments generated in {:?}", stage_started_at.elapsed());

    // fs::write("basic_circuits.bincode", bincode::serialize(&job.basic_circuits).unwrap()).unwrap();
    // fs::write("basic_circuits_inputs.bincode", bincode::serialize(&job.basic_circuits_inputs).unwrap()).unwrap();
    // fs::write("basic_circuits_proofs.bincode", bincode::serialize(&job.basic_circuits_proofs).unwrap()).unwrap();
    // fs::write("vks_for_aggregation.bincode", bincode::serialize(&vks_for_aggregation).unwrap()).unwrap();
    // fs::write("all_vk_committments.bincode", bincode::serialize(&all_vk_committments).unwrap()).unwrap();
    // fs::write("set_committment.bincode", bincode::serialize(&set_committment).unwrap()).unwrap();
    // fs::write("g2_points.bincode", bincode::serialize(&g2_points).unwrap()).unwrap();

    let stage_started_at = Instant::now();

    let (leaf_layer_subqueues, aggregation_outputs, leaf_circuits) =
        zksync_types::zkevm_test_harness::witness::recursive_aggregation::prepare_leaf_aggregations(
            job.basic_circuits,
            job.basic_circuits_inputs,
            job.basic_circuits_proofs,
            vks_for_aggregation,
            LEAF_SPLITTING_FACTOR,
            all_vk_committments,
            set_committment,
            g2_points,
        );

    let serialized_circuits: Vec<(String, Vec<u8>)> =
        witness_generator::serialize_circuits(&leaf_circuits);

    vlog::info!(
        "prepare_leaf_aggregations took {:?}",
        stage_started_at.elapsed()
    );
    vlog::info!(
        "Leaf witness generation for block {} is complete in {:?}. Number of circuits: {}",
        block_number.0,
        started_at.elapsed(),
        leaf_circuits.len()
    );

    LeafAggregationArtifacts {
        leaf_layer_subqueues,
        aggregation_outputs,
        serialized_circuits,
        leaf_circuits,
    }
}

pub fn update_database(
    connection_pool: ConnectionPool,
    started_at: Instant,
    block_number: L1BatchNumber,
    leaf_circuits_len: usize,
    circuits: Vec<String>,
) {
    let mut connection = connection_pool.access_storage_blocking();
    let mut transaction = connection.start_transaction_blocking();

    // inserts artifacts into the node_aggregation_witness_jobs table
    // and advances it to waiting_for_proofs status
    transaction
        .witness_generator_dal()
        .save_leaf_aggregation_artifacts(block_number, leaf_circuits_len);
    transaction.prover_dal().insert_prover_jobs(
        block_number,
        circuits,
        AggregationRound::LeafAggregation,
    );
    transaction
        .witness_generator_dal()
        .mark_witness_job_as_successful(
            block_number,
            AggregationRound::LeafAggregation,
            started_at.elapsed(),
        );

    transaction.commit_blocking();
    track_witness_generation_stage(started_at, AggregationRound::LeafAggregation);
}

pub async fn get_artifacts(
    metadata: WitnessGeneratorJobMetadata,
    object_store: &DynamicObjectStore,
) -> WitnessGeneratorJob {
    let basic_circuits_serialized = object_store
        .get(
            LEAF_AGGREGATION_WITNESS_JOBS_BUCKET_PATH,
            basic_circuits_blob_url(metadata.block_number),
        )
        .unwrap();
    let basic_circuits =
        bincode::deserialize::<BlockBasicCircuits<Bn256>>(&basic_circuits_serialized)
            .expect("basic_circuits deserialization failed");

    let basic_circuits_inputs_serialized = object_store
        .get(
            LEAF_AGGREGATION_WITNESS_JOBS_BUCKET_PATH,
            basic_circuits_inputs_blob_url(metadata.block_number),
        )
        .unwrap();
    let basic_circuits_inputs = bincode::deserialize::<BlockBasicCircuitsPublicInputs<Bn256>>(
        &basic_circuits_inputs_serialized,
    )
    .expect("basic_circuits_inputs deserialization failed");

    WitnessGeneratorJob {
        block_number: metadata.block_number,
        job: WitnessGeneratorJobInput::LeafAggregation(Box::new(
            PrepareLeafAggregationCircuitsJob {
                basic_circuits_inputs,
                basic_circuits_proofs: metadata.proofs,
                basic_circuits,
            },
        )),
    }
}

pub async fn save_artifacts(
    block_number: L1BatchNumber,
    artifacts: LeafAggregationArtifacts,
    object_store: &mut DynamicObjectStore,
) {
    let leaf_layer_subqueues_serialized = bincode::serialize(&artifacts.leaf_layer_subqueues)
        .expect("cannot serialize leaf_layer_subqueues");
    object_store
        .put(
            NODE_AGGREGATION_WITNESS_JOBS_BUCKET_PATH,
            leaf_layer_subqueues_blob_url(block_number),
            leaf_layer_subqueues_serialized,
        )
        .unwrap();

    let aggregation_outputs_serialized = bincode::serialize(&artifacts.aggregation_outputs)
        .expect("cannot serialize aggregation_outputs");
    object_store
        .put(
            NODE_AGGREGATION_WITNESS_JOBS_BUCKET_PATH,
            aggregation_outputs_blob_url(block_number),
            aggregation_outputs_serialized,
        )
        .unwrap();
    save_prover_input_artifacts(
        block_number,
        artifacts.serialized_circuits,
        object_store,
        AggregationRound::LeafAggregation,
    )
    .await;
}
