use std::collections::HashMap;
use std::env;
use std::time::Instant;

use zksync_config::configs::WitnessGeneratorConfig;
use zksync_dal::ConnectionPool;
use zksync_object_store::gcs_utils::{
    aggregation_outputs_blob_url, final_node_aggregations_blob_url, leaf_layer_subqueues_blob_url,
};
use zksync_object_store::object_store::{
    DynamicObjectStore, NODE_AGGREGATION_WITNESS_JOBS_BUCKET_PATH,
    SCHEDULER_WITNESS_JOBS_BUCKET_PATH,
};
use zksync_types::{
    circuit::{
        LEAF_CIRCUIT_INDEX, LEAF_SPLITTING_FACTOR, NODE_CIRCUIT_INDEX, NODE_SPLITTING_FACTOR,
    },
    proofs::{
        AggregationRound, PrepareNodeAggregationCircuitJob, WitnessGeneratorJob,
        WitnessGeneratorJobInput, WitnessGeneratorJobMetadata,
    },
    zkevm_test_harness::{
        abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit,
        bellman::bn256::Bn256,
        bellman::plonk::better_better_cs::setup::VerificationKey,
        ff::to_hex,
        witness::{
            self,
            oracle::VmWitnessOracle,
            recursive_aggregation::{erase_vk_type, padding_aggregations},
        },
        LeafAggregationOutputDataWitness, NodeAggregationOutputDataWitness,
    },
    L1BatchNumber,
};
use zksync_verification_key_server::{
    get_vk_for_circuit_type, get_vks_for_basic_circuits, get_vks_for_commitment,
};

use crate::witness_generator;
use crate::witness_generator::track_witness_generation_stage;
use crate::witness_generator::utils::save_prover_input_artifacts;

pub struct NodeAggregationArtifacts {
    pub final_node_aggregation: NodeAggregationOutputDataWitness<Bn256>,
    pub serialized_circuits: Vec<(String, Vec<u8>)>,
}

pub fn process_node_aggregation_job(
    config: WitnessGeneratorConfig,
    started_at: Instant,
    block_number: L1BatchNumber,
    job: PrepareNodeAggregationCircuitJob,
) -> NodeAggregationArtifacts {
    let stage_started_at = Instant::now();
    zksync_prover_utils::ensure_initial_setup_keys_present(
        &config.initial_setup_key_path,
        &config.key_download_url,
    );
    env::set_var("CRS_FILE", config.initial_setup_key_path);
    vlog::info!("Keys loaded in {:?}", stage_started_at.elapsed());
    let stage_started_at = Instant::now();

    let verification_keys: HashMap<
        u8,
        VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
    > = get_vks_for_basic_circuits();

    let padding_aggregations = padding_aggregations(NODE_SPLITTING_FACTOR);

    let (_, set_committment, g2_points) =
        witness::recursive_aggregation::form_base_circuits_committment(get_vks_for_commitment(
            verification_keys,
        ));

    let node_aggregation_vk = get_vk_for_circuit_type(NODE_CIRCUIT_INDEX);

    let leaf_aggregation_vk = get_vk_for_circuit_type(LEAF_CIRCUIT_INDEX);

    let (_, leaf_aggregation_vk_committment) =
        witness::recursive_aggregation::compute_vk_encoding_and_committment(erase_vk_type(
            leaf_aggregation_vk.clone(),
        ));

    let (_, node_aggregation_vk_committment) =
        witness::recursive_aggregation::compute_vk_encoding_and_committment(erase_vk_type(
            node_aggregation_vk,
        ));

    vlog::info!(
        "commitments: basic set: {:?}, leaf: {:?}, node: {:?}",
        to_hex(&set_committment),
        to_hex(&leaf_aggregation_vk_committment),
        to_hex(&node_aggregation_vk_committment)
    );
    vlog::info!("Commitments generated in {:?}", stage_started_at.elapsed());

    // fs::write("previous_level_proofs.bincode", bincode::serialize(&job.previous_level_proofs).unwrap()).unwrap();
    // fs::write("leaf_aggregation_vk.bincode", bincode::serialize(&leaf_aggregation_vk).unwrap()).unwrap();
    // fs::write("previous_level_leafs_aggregations.bincode", bincode::serialize(&job.previous_level_leafs_aggregations).unwrap()).unwrap();
    // fs::write("previous_sequence.bincode", bincode::serialize(&job.previous_sequence).unwrap()).unwrap();
    // fs::write("padding_aggregations.bincode", bincode::serialize(&padding_aggregations).unwrap()).unwrap();
    // fs::write("set_committment.bincode", bincode::serialize(&set_committment).unwrap()).unwrap();
    // fs::write("node_aggregation_vk_committment.bincode", bincode::serialize(&node_aggregation_vk_committment).unwrap()).unwrap();
    // fs::write("leaf_aggregation_vk_committment.bincode", bincode::serialize(&leaf_aggregation_vk_committment).unwrap()).unwrap();
    // fs::write("g2_points.bincode", bincode::serialize(&g2_points).unwrap()).unwrap();

    let stage_started_at = Instant::now();
    let (_, final_node_aggregations, node_circuits) =
        zksync_types::zkevm_test_harness::witness::recursive_aggregation::prepare_node_aggregations(
            job.previous_level_proofs,
            leaf_aggregation_vk,
            true,
            0,
            job.previous_level_leafs_aggregations,
            Vec::default(),
            job.previous_sequence,
            LEAF_SPLITTING_FACTOR,
            NODE_SPLITTING_FACTOR,
            padding_aggregations,
            set_committment,
            node_aggregation_vk_committment,
            leaf_aggregation_vk_committment,
            g2_points,
        );

    vlog::info!(
        "prepare_node_aggregations took {:?}",
        stage_started_at.elapsed()
    );

    assert_eq!(
        node_circuits.len(),
        1,
        "prepare_node_aggregations returned more than one circuit"
    );
    assert_eq!(
        final_node_aggregations.len(),
        1,
        "prepare_node_aggregations returned more than one node aggregation"
    );

    let serialized_circuits: Vec<(String, Vec<u8>)> =
        witness_generator::serialize_circuits(&node_circuits);

    vlog::info!(
        "Node witness generation for block {} is complete in {:?}. Number of circuits: {}",
        block_number.0,
        started_at.elapsed(),
        node_circuits.len()
    );

    NodeAggregationArtifacts {
        final_node_aggregation: final_node_aggregations.into_iter().next().unwrap(),
        serialized_circuits,
    }
}

pub fn update_database(
    connection_pool: ConnectionPool,
    started_at: Instant,
    block_number: L1BatchNumber,
    circuits: Vec<String>,
) {
    let mut connection = connection_pool.access_storage_blocking();
    let mut transaction = connection.start_transaction_blocking();

    // inserts artifacts into the scheduler_witness_jobs table
    // and advances it to waiting_for_proofs status
    transaction
        .witness_generator_dal()
        .save_node_aggregation_artifacts(block_number);
    transaction.prover_dal().insert_prover_jobs(
        block_number,
        circuits,
        AggregationRound::NodeAggregation,
    );
    transaction
        .witness_generator_dal()
        .mark_witness_job_as_successful(
            block_number,
            AggregationRound::NodeAggregation,
            started_at.elapsed(),
        );

    transaction.commit_blocking();
    track_witness_generation_stage(started_at, AggregationRound::NodeAggregation);
}

pub async fn get_artifacts(
    metadata: WitnessGeneratorJobMetadata,
    object_store: &DynamicObjectStore,
) -> WitnessGeneratorJob {
    let leaf_layer_subqueues_serialized = object_store
        .get(
            NODE_AGGREGATION_WITNESS_JOBS_BUCKET_PATH,
            leaf_layer_subqueues_blob_url(metadata.block_number),
        )
        .expect(
            "leaf_layer_subqueues is not found in a `queued` `node_aggregation_witness_jobs` job",
        );
    let leaf_layer_subqueues = bincode::deserialize::<
        Vec<
            zksync_types::zkevm_test_harness::encodings::QueueSimulator<
                Bn256,
                zksync_types::zkevm_test_harness::encodings::recursion_request::RecursionRequest<
                    Bn256,
                >,
                2,
                2,
            >,
        >,
    >(&leaf_layer_subqueues_serialized)
    .expect("leaf_layer_subqueues deserialization failed");

    let aggregation_outputs_serialized = object_store
        .get(
            NODE_AGGREGATION_WITNESS_JOBS_BUCKET_PATH,
            aggregation_outputs_blob_url(metadata.block_number),
        )
        .expect(
            "aggregation_outputs is not found in a `queued` `node_aggregation_witness_jobs` job",
        );
    let aggregation_outputs = bincode::deserialize::<Vec<LeafAggregationOutputDataWitness<Bn256>>>(
        &aggregation_outputs_serialized,
    )
    .expect("aggregation_outputs deserialization failed");

    WitnessGeneratorJob {
        block_number: metadata.block_number,
        job: WitnessGeneratorJobInput::NodeAggregation(Box::new(
            PrepareNodeAggregationCircuitJob {
                previous_level_proofs: metadata.proofs,
                previous_level_leafs_aggregations: aggregation_outputs,
                previous_sequence: leaf_layer_subqueues,
            },
        )),
    }
}

pub async fn save_artifacts(
    block_number: L1BatchNumber,
    artifacts: NodeAggregationArtifacts,
    object_store: &mut DynamicObjectStore,
) {
    let final_node_aggregations_serialized = bincode::serialize(&artifacts.final_node_aggregation)
        .expect("cannot serialize final_node_aggregations");

    object_store
        .put(
            SCHEDULER_WITNESS_JOBS_BUCKET_PATH,
            final_node_aggregations_blob_url(block_number),
            final_node_aggregations_serialized,
        )
        .unwrap();
    save_prover_input_artifacts(
        block_number,
        artifacts.serialized_circuits,
        object_store,
        AggregationRound::NodeAggregation,
    )
    .await;
}
