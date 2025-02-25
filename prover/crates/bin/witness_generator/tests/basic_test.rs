use std::time::{Duration, Instant};

use serde::Serialize;
use zksync_config::{configs::object_store::ObjectStoreMode, ObjectStoreConfig};
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_prover_fri_types::{
    keys::{AggregationsKey, FriCircuitKey},
    CircuitWrapper,
};
use zksync_prover_fri_utils::get_recursive_layer_circuit_id_for_base_layer;
use zksync_prover_keystore::keystore::Keystore;
use zksync_types::{
    basic_fri_types::AggregationRound,
    prover_dal::{LeafAggregationJobMetadata, NodeAggregationJobMetadata},
    L1BatchNumber,
};
use zksync_witness_generator::{
    rounds::{JobManager, LeafAggregation, NodeAggregation},
    utils::AggregationWrapper,
};

/// Maximum allowed execution time for each test case
const MAX_EXECUTION_TIME: Duration = Duration::from_secs(10);
/// List of circuit IDs that are valid for testing
const VALID_CIRCUIT_IDS: &[u8] = &[1, 4, 8, 13];

/// Paths for test data and expected results
const LEAF_TEST_DATA_PATH: &str = "./tests/data/leaf/";
const NODE_TEST_DATA_PATH: &str = "./tests/data/node/";
const EXPECTED_LEAF_DATA_PATH: &str = "./tests/expected_data/leaf/";
const EXPECTED_NODE_DATA_PATH: &str = "./tests/expected_data/node/";

/// Compares two serializable values for equality
/// 
/// # Arguments
/// * `expected` - The expected value
/// * `actual` - The actual value to compare against
/// 
/// # Panics
/// Panics if the serialized representations don't match
fn compare_serialized<T: Serialize>(expected: &T, actual: &T) {
    let serialized_expected = bincode::serialize(expected).unwrap();
    let serialized_actual = bincode::serialize(actual).unwrap();
    assert_eq!(
        serialized_expected, 
        serialized_actual,
        "Serialized data mismatch - expected and actual values differ"
    );
}

/// Creates a test object store with the given base path
/// 
/// # Arguments
/// * `base_path` - Base path for the object store
/// 
/// # Returns
/// A new object store instance
async fn create_test_object_store(base_path: &str) -> Box<dyn ObjectStore> {
    let config = ObjectStoreConfig {
        mode: ObjectStoreMode::FileBacked {
            file_backed_base_path: base_path.to_owned(),
        },
        max_retries: 5,
        local_mirror_path: None,
    };
    ObjectStoreFactory::new(config)
        .create_store()
        .await
        .expect("Failed to create object store")
}

/// Tests leaf witness generation with different circuit IDs
/// 
/// This test verifies that:
/// - Leaf witness generation works for all valid circuit IDs
/// - Generated artifacts match expected results
/// - Processing completes within time limits
/// - Circuit IDs are correctly preserved
#[tokio::test]
#[ignore] // re-enable with new artifacts
async fn test_leaf_witness_gen_parameterized() {
    for &circuit_id in VALID_CIRCUIT_IDS {
        println!("Testing leaf witness generation for circuit_id: {}", circuit_id);
        let start_time = Instant::now();
        
        let block_number = L1BatchNumber(125010);
        let leaf_aggregation_job_metadata = LeafAggregationJobMetadata {
            id: 1,
            block_number,
            circuit_id,
            prover_job_ids_for_proofs: vec![4639043, 4639044, 4639045],
        };

        let object_store = create_test_object_store(LEAF_TEST_DATA_PATH).await;
        let keystore = Keystore::locate();
        
        let job = LeafAggregation::prepare_job(leaf_aggregation_job_metadata, &*object_store, keystore)
            .await
            .expect("Failed to prepare leaf aggregation job");

        let artifacts = LeafAggregation::process_job(job, object_store.clone(), 500, Instant::now())
            .await
            .expect("Failed to process leaf aggregation job");

        let aggregations = AggregationWrapper(artifacts.aggregations);
        let expected_object_store = create_test_object_store(EXPECTED_LEAF_DATA_PATH).await;

        for (idx, circuit_metadata) in artifacts.circuit_ids_and_urls.into_iter().enumerate() {
            assert_eq!(
                circuit_id, 
                circuit_metadata.0,
                "Circuit ID mismatch for index {}", 
                idx
            );

            let circuit_key = FriCircuitKey {
                block_number,
                sequence_number: idx,
                circuit_id,
                aggregation_round: AggregationRound::LeafAggregation,
                depth: 0,
            };

            let result = object_store
                .get::<CircuitWrapper>(circuit_key)
                .await
                .unwrap_or_else(|_| panic!("Failed to get result circuit for index {}", idx));

            let expected_result = expected_object_store
                .get::<CircuitWrapper>(circuit_key)
                .await
                .unwrap_or_else(|_| panic!("Failed to get expected circuit for index {}", idx));

            compare_serialized(&expected_result, &result);
        }

        let agg_key = AggregationsKey {
            block_number,
            circuit_id: get_recursive_layer_circuit_id_for_base_layer(circuit_id),
            depth: 0,
        };
        
        let expected_aggregation = expected_object_store
            .get::<AggregationWrapper>(agg_key)
            .await
            .expect("Failed to get expected aggregation");

        compare_serialized(&expected_aggregation, &aggregations);

        let execution_time = start_time.elapsed();
        assert!(
            execution_time <= MAX_EXECUTION_TIME,
            "Test execution took too long: {:?} (max allowed: {:?})",
            execution_time,
            MAX_EXECUTION_TIME
        );
    }
}

/// Tests node witness generation with different circuit IDs
/// 
/// This test verifies that:
/// - Node witness generation works for all valid circuit IDs
/// - Generated artifacts match expected results
/// - Processing completes within time limits
/// - Circuit IDs are correctly preserved in recursive processing
#[tokio::test]
#[ignore] // re-enable with new artifacts
async fn test_node_witness_gen_parameterized() {
    for &circuit_id in VALID_CIRCUIT_IDS {
        println!("Testing node witness generation for circuit_id: {}", circuit_id);
        let start_time = Instant::now();

        let object_store = create_test_object_store(NODE_TEST_DATA_PATH).await;
        let block_number = L1BatchNumber(127856);

        let node_aggregation_job_metadata = NodeAggregationJobMetadata {
            id: 1,
            block_number,
            circuit_id,
            depth: 0,
            prover_job_ids_for_proofs: vec![5211320],
        };

        let keystore = Keystore::locate();
        let job = NodeAggregation::prepare_job(node_aggregation_job_metadata, &*object_store, keystore)
            .await
            .expect("Failed to prepare node aggregation job");

        let artifacts = NodeAggregation::process_job(job, object_store.clone(), 500, Instant::now())
            .await
            .expect("Failed to process node aggregation job");

        let aggregations = AggregationWrapper(artifacts.next_aggregations);
        let expected_object_store = create_test_object_store(EXPECTED_NODE_DATA_PATH).await;

        for (idx, circuit_metadata) in artifacts
            .recursive_circuit_ids_and_urls
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                circuit_id, 
                circuit_metadata.0,
                "Circuit ID mismatch for index {}", 
                idx
            );

            let circuit_key = FriCircuitKey {
                block_number,
                sequence_number: idx,
                circuit_id,
                aggregation_round: AggregationRound::NodeAggregation,
                depth: 0,
            };

            let result = object_store
                .get::<CircuitWrapper>(circuit_key)
                .await
                .unwrap_or_else(|_| panic!("Failed to get result circuit for index {}", idx));

            let expected_result = expected_object_store
                .get::<CircuitWrapper>(circuit_key)
                .await
                .unwrap_or_else(|_| panic!("Failed to get expected circuit for index {}", idx));

            compare_serialized(&expected_result, &result);
        }

        let agg_key = AggregationsKey {
            block_number,
            circuit_id: get_recursive_layer_circuit_id_for_base_layer(circuit_id),
            depth: 1,
        };
        
        let expected_aggregation = expected_object_store
            .get::<AggregationWrapper>(agg_key)
            .await
            .expect("Failed to get expected aggregation");

        compare_serialized(&expected_aggregation, &aggregations);

        let execution_time = start_time.elapsed();
        assert!(
            execution_time <= MAX_EXECUTION_TIME,
            "Test execution took too long: {:?} (max allowed: {:?})",
            execution_time,
            MAX_EXECUTION_TIME
        );
    }
}

/// Tests error handling in witness generation
/// 
/// This test verifies that the system properly handles:
/// - Invalid circuit IDs
/// - Empty prover job IDs
/// - Other error conditions that should be caught early
#[tokio::test]
async fn test_error_handling() {
    // Test with invalid circuit ID
    let invalid_circuit_id = 255;
    let block_number = L1BatchNumber(125010);
    
    let leaf_aggregation_job_metadata = LeafAggregationJobMetadata {
        id: 1,
        block_number,
        circuit_id: invalid_circuit_id,
        prover_job_ids_for_proofs: vec![4639043],
    };

    let object_store = create_test_object_store(LEAF_TEST_DATA_PATH).await;
    let keystore = Keystore::locate();
    
    let job_result = LeafAggregation::prepare_job(leaf_aggregation_job_metadata, &*object_store, keystore).await;
    assert!(
        job_result.is_err(),
        "Expected error for invalid circuit ID {}, but got success",
        invalid_circuit_id
    );

    // Test with empty prover job IDs
    let leaf_aggregation_job_metadata = LeafAggregationJobMetadata {
        id: 1,
        block_number,
        circuit_id: VALID_CIRCUIT_IDS[0],
        prover_job_ids_for_proofs: vec![],
    };

    let job_result = LeafAggregation::prepare_job(leaf_aggregation_job_metadata, &*object_store, keystore).await;
    assert!(
        job_result.is_err(),
        "Expected error for empty prover job IDs, but got success"
    );
}

/// Tests parallel execution of witness generation
/// 
/// This test verifies that multiple witness generations can run
/// concurrently without interfering with each other
#[tokio::test]
async fn test_parallel_execution() {
    let block_number = L1BatchNumber(125010);
    let mut handles = vec![];

    // Start multiple leaf witness generations concurrently
    for &circuit_id in VALID_CIRCUIT_IDS {
        let handle = tokio::spawn(async move {
            let leaf_aggregation_job_metadata = LeafAggregationJobMetadata {
                id: 1,
                block_number,
                circuit_id,
                prover_job_ids_for_proofs: vec![4639043, 4639044, 4639045],
            };

            let object_store = create_test_object_store(LEAF_TEST_DATA_PATH).await;
            let keystore = Keystore::locate();
            
            let job = LeafAggregation::prepare_job(leaf_aggregation_job_metadata, &*object_store, keystore)
                .await
                .expect("Failed to prepare leaf aggregation job");

            LeafAggregation::process_job(job, object_store.clone(), 500, Instant::now())
                .await
                .expect("Failed to process leaf aggregation job");
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.expect("Failed to execute parallel test");
    }
}
