use std::time::Instant;

use serde::Serialize;
use zksync_config::ObjectStoreConfig;
use zksync_env_config::FromEnv;
use zksync_object_store::{AggregationsKey, FriCircuitKey, ObjectStoreFactory};
use zksync_prover_fri_types::CircuitWrapper;
use zksync_prover_fri_utils::get_recursive_layer_circuit_id_for_base_layer;
use zksync_types::{
    proofs::{AggregationRound, LeafAggregationJobMetadata, NodeAggregationJobMetadata},
    L1BatchNumber,
};
use zksync_witness_generator::{
    leaf_aggregation::{prepare_leaf_aggregation_job, LeafAggregationWitnessGenerator},
    node_aggregation,
    node_aggregation::NodeAggregationWitnessGenerator,
    scheduler,
    scheduler::SchedulerWitnessGenerator,
    utils::AggregationWrapper,
};

fn compare_serialized<T: Serialize>(expected: &T, actual: &T) {
    let serialized_expected = bincode::serialize(expected).unwrap();
    let serialized_actual = bincode::serialize(actual).unwrap();
    assert_eq!(serialized_expected, serialized_actual);
}

#[tokio::test]
#[ignore] // re-enable with new artifacts
async fn test_leaf_witness_gen() {
    let mut object_store_config = ObjectStoreConfig::from_env().unwrap();
    object_store_config.file_backed_base_path = "./tests/data/leaf/".to_owned();
    let object_store = ObjectStoreFactory::new(object_store_config)
        .create_store()
        .await;

    let circuit_id = 4;
    let block_number = L1BatchNumber(125010);
    let key = AggregationsKey {
        block_number,
        circuit_id: get_recursive_layer_circuit_id_for_base_layer(circuit_id),
        depth: 0,
    };
    let expected_aggregation = object_store
        .get::<AggregationWrapper>(key)
        .await
        .expect("expected aggregation missing");
    let leaf_aggregation_job_metadata = LeafAggregationJobMetadata {
        id: 1,
        block_number,
        circuit_id,
        prover_job_ids_for_proofs: vec![4639043, 4639044, 4639045],
    };

    let job = prepare_leaf_aggregation_job(leaf_aggregation_job_metadata, &*object_store)
        .await
        .unwrap();

    let artifacts = LeafAggregationWitnessGenerator::process_job_sync(job, Instant::now());
    let aggregations = AggregationWrapper(artifacts.aggregations);
    compare_serialized(&expected_aggregation, &aggregations);
}

#[tokio::test]
#[ignore] // re-enable with new artifacts
async fn test_node_witness_gen() {
    let mut object_store_config = ObjectStoreConfig::from_env().unwrap();
    object_store_config.file_backed_base_path = "./tests/data/node/".to_owned();
    let object_store = ObjectStoreFactory::new(object_store_config)
        .create_store()
        .await;

    let circuit_id = 8;
    let block_number = L1BatchNumber(127856);
    let key = AggregationsKey {
        block_number,
        circuit_id,
        depth: 1,
    };
    let expected_aggregation = object_store
        .get::<AggregationWrapper>(key)
        .await
        .expect("expected aggregation missing");
    let node_aggregation_job_metadata = NodeAggregationJobMetadata {
        id: 1,
        block_number,
        circuit_id,
        depth: 0,
        prover_job_ids_for_proofs: vec![5211320],
    };

    let job = node_aggregation::prepare_job(node_aggregation_job_metadata, &*object_store)
        .await
        .unwrap();

    let artifacts = NodeAggregationWitnessGenerator::process_job_sync(job, Instant::now());
    let aggregations = AggregationWrapper(artifacts.next_aggregations);
    compare_serialized(&expected_aggregation, &aggregations);
}

#[tokio::test]
#[ignore] // re-enable with new artifacts
async fn test_scheduler_witness_gen() {
    let mut object_store_config = ObjectStoreConfig::from_env().unwrap();
    object_store_config.file_backed_base_path = "./tests/data/scheduler/".to_owned();
    let object_store = ObjectStoreFactory::new(object_store_config)
        .create_store()
        .await;
    let block_number = L1BatchNumber(128599);
    let key = FriCircuitKey {
        block_number,
        circuit_id: 1,
        sequence_number: 0,
        depth: 0,
        aggregation_round: AggregationRound::Scheduler,
    };
    let expected_circuit = object_store
        .get(key)
        .await
        .expect("expected scheduler circuit missing");
    let proof_job_ids = [
        5639969, 5627082, 5627084, 5627083, 5627086, 5627085, 5631320, 5627090, 5627091, 5627092,
        5627093, 5627094, 5629097,
    ];

    let job = scheduler::prepare_job(block_number, proof_job_ids, &*object_store)
        .await
        .unwrap();

    let artifacts = SchedulerWitnessGenerator::process_job_sync(job, Instant::now());
    let circuit = CircuitWrapper::Recursive(artifacts.scheduler_circuit);
    compare_serialized(&expected_circuit, &circuit);
}
