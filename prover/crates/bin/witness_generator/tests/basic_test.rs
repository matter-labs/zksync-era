use std::time::Instant;

use serde::Serialize;
use zksync_config::{configs::object_store::ObjectStoreMode, ObjectStoreConfig};
use zksync_object_store::ObjectStoreFactory;
use zksync_prover_fri_types::{
    keys::{AggregationsKey, FriCircuitKey},
    CircuitWrapper,
};
use zksync_prover_fri_utils::get_recursive_layer_circuit_id_for_base_layer;
use zksync_types::{
    basic_fri_types::AggregationRound,
    prover_dal::{LeafAggregationJobMetadata, NodeAggregationJobMetadata},
    L1BatchNumber,
};
use zksync_witness_generator::{
    leaf_aggregation::{prepare_leaf_aggregation_job, LeafAggregationWitnessGenerator},
    node_aggregation::{self, NodeAggregationWitnessGenerator},
    utils::AggregationWrapper,
    basic_circuits::generate_witness
};

fn compare_serialized<T: Serialize>(expected: &T, actual: &T) {
    let serialized_expected = bincode::serialize(expected).unwrap();
    let serialized_actual = bincode::serialize(actual).unwrap();
    assert_eq!(serialized_expected, serialized_actual);
}

#[tokio::test]
#[ignore] // re-enable with new artifacts
async fn test_leaf_witness_gen() {
    let circuit_id = 4;
    let block_number = L1BatchNumber(125010);

    let leaf_aggregation_job_metadata = LeafAggregationJobMetadata {
        id: 1,
        block_number,
        circuit_id,
        prover_job_ids_for_proofs: vec![4639043, 4639044, 4639045],
    };

    let object_store_config = ObjectStoreConfig {
        mode: ObjectStoreMode::FileBacked {
            file_backed_base_path: "./tests/data/leaf/".to_owned(),
        },
        max_retries: 5,
        local_mirror_path: None,
    };
    let object_store = ObjectStoreFactory::new(object_store_config)
        .create_store()
        .await
        .unwrap();

    let job = prepare_leaf_aggregation_job(
        leaf_aggregation_job_metadata,
        &*object_store,
        "crates/bin/vk_setup_data_generator/data".to_string(),
    )
    .await
    .unwrap();

    let artifacts = LeafAggregationWitnessGenerator::process_job_impl(
        job,
        Instant::now(),
        object_store.clone(),
        500,
    )
    .await;

    let aggregations = AggregationWrapper(artifacts.aggregations);

    let expected_results_object_store_config = ObjectStoreConfig {
        mode: ObjectStoreMode::FileBacked {
            file_backed_base_path: "./tests/expected_data/leaf/".to_owned(),
        },
        max_retries: 5,
        local_mirror_path: None,
    };
    let expected_object_store = ObjectStoreFactory::new(expected_results_object_store_config)
        .create_store()
        .await
        .unwrap();

    for (idx, circuit_metadata) in artifacts.circuit_ids_and_urls.into_iter().enumerate() {
        assert_eq!(circuit_id, circuit_metadata.0);

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
            .unwrap_or_else(|_| panic!("result circuit missing: {}", idx));

        let expected_result = expected_object_store
            .get::<CircuitWrapper>(circuit_key)
            .await
            .unwrap_or_else(|_| panic!("expected circuit missing: {}", idx));

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
        .expect("expected aggregation missing");

    compare_serialized(&expected_aggregation, &aggregations);
}

#[tokio::test]
#[ignore] // re-enable with new artifacts
async fn test_node_witness_gen() {
    let object_store_config = ObjectStoreConfig {
        mode: ObjectStoreMode::FileBacked {
            file_backed_base_path: "./tests/data/node/".to_owned(),
        },
        max_retries: 5,
        local_mirror_path: None,
    };
    let object_store = ObjectStoreFactory::new(object_store_config)
        .create_store()
        .await
        .unwrap();

    let circuit_id = 8;
    let block_number = L1BatchNumber(127856);

    let node_aggregation_job_metadata = NodeAggregationJobMetadata {
        id: 1,
        block_number,
        circuit_id,
        depth: 0,
        prover_job_ids_for_proofs: vec![5211320],
    };

    let job = node_aggregation::prepare_job(
        node_aggregation_job_metadata,
        &*object_store,
        "crates/bin/vk_setup_data_generator/data".to_string(),
    )
    .await
    .unwrap();

    let artifacts = NodeAggregationWitnessGenerator::process_job_impl(
        job,
        Instant::now(),
        object_store.clone(),
        500,
    )
    .await;
    let aggregations = AggregationWrapper(artifacts.next_aggregations);

    let expected_results_object_store_config = ObjectStoreConfig {
        mode: ObjectStoreMode::FileBacked {
            file_backed_base_path: "./tests/expected_data/leaf/".to_owned(),
        },
        max_retries: 5,
        local_mirror_path: None,
    };
    let expected_object_store = ObjectStoreFactory::new(expected_results_object_store_config)
        .create_store()
        .await
        .unwrap();

    for (idx, circuit_metadata) in artifacts
        .recursive_circuit_ids_and_urls
        .into_iter()
        .enumerate()
    {
        assert_eq!(circuit_id, circuit_metadata.0);

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
            .unwrap_or_else(|_| panic!("result circuit missing: {}", idx));

        let expected_result = expected_object_store
            .get::<CircuitWrapper>(circuit_key)
            .await
            .unwrap_or_else(|_| panic!("expected circuit missing: {}", idx));

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
        .expect("expected aggregation missing");

    compare_serialized(&expected_aggregation, &aggregations);
}


#[tokio::test]
async fn test_basic_witness_gen() {
    let object_store_config = ObjectStoreConfig {
        mode: ObjectStoreMode::FileBacked {
            file_backed_base_path: "./tests/data/bwg/".to_owned(),
        },
        max_retries: 5,
        local_mirror_path: None,
    };
    let object_store = ObjectStoreFactory::new(object_store_config)
        .create_store()
        .await
        .unwrap();

    //let block_number = L1BatchNumber(489509);
    let block_number = L1BatchNumber(11163);

    let input = object_store.get(block_number).await.unwrap();

    let instant = Instant::now();

    //snapshot_prof("Before run");
    let (_circuit_urls, _queue_urls, _scheduler_witness, _aux_output_witness) =
        generate_witness(block_number, object_store, input, 500).await;
    //snapshot_prof("After run");
    println!("Generated witness, {:?}", instant.elapsed());
    //print_peak_mem_snapshots();
}
