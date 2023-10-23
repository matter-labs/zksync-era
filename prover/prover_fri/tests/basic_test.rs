use anyhow::Context as _;
use std::sync::Arc;

use zksync_config::configs::FriProverConfig;
use zksync_config::ObjectStoreConfig;
use zksync_object_store::{bincode, FriCircuitKey, ObjectStoreFactory};
use zksync_types::proofs::AggregationRound;
use zksync_types::L1BatchNumber;

use serde::Serialize;
use zksync_prover_fri::prover_job_processor::Prover;
use zksync_prover_fri_types::{CircuitWrapper, ProverJob, ProverServiceDataKey};
use zksync_vk_setup_data_server_fri::generate_cpu_base_layer_setup_data;

fn compare_serialized<T: Serialize>(expected: &T, actual: &T) {
    let serialized_expected = bincode::serialize(expected).unwrap();
    let serialized_actual = bincode::serialize(actual).unwrap();
    assert_eq!(serialized_expected, serialized_actual);
}

async fn prover_and_assert_base_layer(
    expected_proof_id: u32,
    circuit_id: u8,
    block_number: L1BatchNumber,
    sequence_number: usize,
) -> anyhow::Result<()> {
    let mut object_store_config =
        ObjectStoreConfig::from_env().context("ObjectStoreConfig::from_env()")?;
    object_store_config.file_backed_base_path = "./tests/data/".to_owned();
    let object_store = ObjectStoreFactory::new(object_store_config)
        .create_store()
        .await;
    let expected_proof = object_store
        .get(expected_proof_id)
        .await
        .expect("missing expected proof");

    let aggregation_round = AggregationRound::BasicCircuits;
    let blob_key = FriCircuitKey {
        block_number,
        circuit_id,
        sequence_number,
        depth: 0,
        aggregation_round,
    };
    let circuit_wrapper = object_store
        .get(blob_key)
        .await
        .context("circuit missing")?;
    let circuit = match &circuit_wrapper {
        CircuitWrapper::Base(base) => base.clone(),
        CircuitWrapper::Recursive(_) => anyhow::bail!("Expected base layer circuit"),
    };
    let setup_data = Arc::new(
        generate_cpu_base_layer_setup_data(circuit)
            .context("generate_cpu_base_layers_setup_data()")?,
    );
    let setup_key = ProverServiceDataKey::new(circuit_id, aggregation_round);
    let prover_job = ProverJob::new(block_number, expected_proof_id, circuit_wrapper, setup_key);
    let artifacts = Prover::prove(
        prover_job,
        Arc::new(FriProverConfig::from_env().context("FriProverConfig::from_env()")?),
        setup_data,
    );
    compare_serialized(&expected_proof, &artifacts.proof_wrapper);
    Ok(())
}

// #[tokio::test]
// async fn test_base_layer_main_vm_proof_gen() {
//     prover_and_assert_base_layer(5176866, 1, L1BatchNumber(128623), 1086).await;
// }

#[tokio::test]
async fn test_base_layer_sha256_proof_gen() {
    prover_and_assert_base_layer(1293714, 6, L1BatchNumber(114499), 479)
        .await
        .unwrap();
}
