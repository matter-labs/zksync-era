#![feature(generic_const_exprs)]

use std::time::Instant;

use serde::Serialize;
use zksync_config::{configs::object_store::ObjectStoreMode, ObjectStoreConfig};
use zksync_object_store::ObjectStoreFactory;
use zksync_prover_fri_types::keys::AggregationsKey;
use zksync_prover_fri_utils::get_recursive_layer_circuit_id_for_base_layer;
use zksync_types::{
    prover_dal::{LeafAggregationJobMetadata, NodeAggregationJobMetadata},
    L1BatchNumber,
};
use zksync_witness_generator::{
    leaf_aggregation::{prepare_leaf_aggregation_job, LeafAggregationWitnessGenerator},
    node_aggregation,
    node_aggregation::NodeAggregationWitnessGenerator,
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
async fn manual_batch_witness_generation() {
    let object_store_config = ObjectStoreConfig {
        mode: ObjectStoreMode::FileBacked {
            file_backed_base_path: "./tests/data/basic/".to_owned(),
        },
        max_retries: 5,
        local_mirror_path: None,
    };

    let object_store = ObjectStoreFactory::new(object_store_config)
        .create_store()
        .await
        .unwrap();

    use std::str::FromStr;
    /*let max_connections = 10;
    let connection_pool = ConnectionPool::<Core>::builder(connection_url, max_connections)
        .build()
        .await
        .context("failed to build a connection_pool")
        .unwrap();
    */
    use std::{fs, fs::File};

    use anyhow::Context;
    use ciborium;
    use prover_dal::Prover;
    use zksync_dal::{ConnectionPool, Core};
    use zksync_types::url::SensitiveUrl;

    let eip_4844_blobs_res = File::open("./tests/data/basic/blobs.bin");
    let witness_gen_input_res = File::open("./tests/data/basic/witness_gen_input.bin");
    let inputs_for_witness_generation_res =
        File::open("./tests/data/basic/inputs_for_witness_generation.bin");

    let (block_number, inputs_for_witness_generation, witness_gen_input, eip_4844_blobs) =
        if eip_4844_blobs_res.is_err()
            || witness_gen_input_res.is_err()
            || inputs_for_witness_generation_res.is_err()
        {
            panic!("No artfacts for tests!");

        /*let batch_number = 487023;

        let prover_connection_pool = ConnectionPool::<Prover>::singleton(prover_url)
            .build()
            .await
            .context("failed to build a prover_connection_pool")
            .unwrap();

        use prover_dal::ProverDal;
        let job = prover_connection_pool
            .connection()
            .await
            .unwrap()
            .fri_witness_generator_dal()
            .get_basic_witness_generator_job_for_batch(L1BatchNumber(batch_number))
            .await
            .unwrap();

        use zksync_types::prover_dal::BasicWitnessGeneratorJobInfo;
        let BasicWitnessGeneratorJobInfo {
            eip_4844_blobs,
            l1_batch_number,
            ..
        } = job;

        use zksync_witness_generator::basic_circuits::get_artifacts;
        let basic_job = get_artifacts(
            l1_batch_number,
            &*object_store,
            eip_4844_blobs.unwrap().clone(),
        )
        .await; // BasicWitnessGeneratorJob

        use zksync_witness_generator::basic_circuits::BasicWitnessGeneratorJob;
        let BasicWitnessGeneratorJob {
            block_number,
            job,
            eip_4844_blobs,
        } = basic_job;
        let mut buffer = File::create("./tests/data/basic/blobs.bin").unwrap();
        ciborium::into_writer(&eip_4844_blobs, buffer).unwrap();

        use zksync_witness_generator::basic_circuits::build_basic_circuits_witness_generator_input;
        let witness_gen_input = build_basic_circuits_witness_generator_input(
            &connection_pool,
            job,
            l1_batch_number,
        )
        .await;
        let mut buffer = File::create("./tests/data/basic/witness_gen_input.bin").unwrap();
        ciborium::into_writer(&witness_gen_input, buffer).unwrap();

        use zksync_witness_generator::basic_circuits::prepare_inputs_for_artifacts_generation;

        let inputs_for_witness_generation = prepare_inputs_for_artifacts_generation(
            &connection_pool,
            witness_gen_input.clone(),
        )
        .await;

        let mut buffer =
            File::create("./tests/data/basic/inputs_for_witness_generation.bin").unwrap();
        ciborium::into_writer(&inputs_for_witness_generation, buffer).unwrap();

        (
            block_number,
            inputs_for_witness_generation,
            witness_gen_input,
            eip_4844_blobs,
        )
        */
        } else {
            use zksync_prover_interface::inputs::BasicCircuitWitnessGeneratorInput;
            let eip_4844_blobs = ciborium::from_reader(&eip_4844_blobs_res.unwrap()).unwrap();
            let witness_gen_input: BasicCircuitWitnessGeneratorInput =
                ciborium::from_reader(&witness_gen_input_res.unwrap()).unwrap();
            let inputs_for_witness_generation =
                ciborium::from_reader(&inputs_for_witness_generation_res.unwrap()).unwrap();

            (
                witness_gen_input.block_number,
                inputs_for_witness_generation,
                witness_gen_input,
                eip_4844_blobs,
            )
        };

    use zksync_witness_generator::basic_circuits::generate_witness;
    let (_circuit_urls, _queue_urls, _scheduler_witness, _aux_output_witness) = generate_witness(
        block_number,
        &*object_store,
        inputs_for_witness_generation,
        witness_gen_input,
        eip_4844_blobs,
    )
    .await;
}
