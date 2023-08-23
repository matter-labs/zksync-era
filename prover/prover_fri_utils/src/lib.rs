pub mod socket_utils;

use std::time::Instant;

use zksync_config::configs::fri_prover_group::CircuitIdRoundTuple;
use zksync_dal::fri_prover_dal::FriProverDal;
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::base_circuit_type_into_recursive_leaf_circuit_type;
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerStorageType;
use zksync_prover_fri_types::circuit_definitions::zkevm_circuits::scheduler::aux::BaseLayerCircuitType;

use zksync_object_store::{FriCircuitKey, ObjectStore};
use zksync_prover_fri_types::{CircuitWrapper, ProverJob, ProverServiceDataKey};

pub async fn fetch_next_circuit(
    fri_prover_dal: &mut FriProverDal<'_, '_>,
    blob_store: &dyn ObjectStore,
    circuit_ids_for_round_to_be_proven: &Vec<CircuitIdRoundTuple>,
) -> Option<ProverJob> {
    let prover_job = match &circuit_ids_for_round_to_be_proven.is_empty() {
        false => {
            // Specialized prover: proving subset of configured circuits.
            fri_prover_dal
                .get_next_job_for_circuit_id_round(&circuit_ids_for_round_to_be_proven)
                .await
        }
        true => {
            // Generalized prover: proving all circuits.
            fri_prover_dal.get_next_job().await
        }
    }?;
    vlog::info!("Started processing prover job: {:?}", prover_job);

    let circuit_key = FriCircuitKey {
        block_number: prover_job.block_number,
        sequence_number: prover_job.sequence_number,
        circuit_id: prover_job.circuit_id,
        aggregation_round: prover_job.aggregation_round,
        depth: prover_job.depth,
    };
    let started_at = Instant::now();
    let input = blob_store
        .get(circuit_key)
        .await
        .unwrap_or_else(|err| panic!("{err:?}"));
    metrics::histogram!(
                "prover_fri.prover.blob_fetch_time",
                started_at.elapsed(),
                "circuit_type" => prover_job.circuit_id.to_string(),
                "aggregation_round" => format!("{:?}", prover_job.aggregation_round),
    );
    let setup_data_key = ProverServiceDataKey {
        circuit_id: prover_job.circuit_id,
        round: prover_job.aggregation_round,
    };
    Some(ProverJob::new(
        prover_job.block_number,
        prover_job.id,
        input,
        setup_data_key,
    ))
}

pub fn get_recursive_layer_circuit_id_for_base_layer(base_layer_circuit_id: u8) -> u8 {
    let recursive_circuit_type = base_circuit_type_into_recursive_leaf_circuit_type(
        BaseLayerCircuitType::from_numeric_value(base_layer_circuit_id),
    );
    recursive_circuit_type as u8
}

pub fn get_base_layer_circuit_id_for_recursive_layer(recursive_layer_circuit_id: u8) -> u8 {
    recursive_layer_circuit_id - ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8
}

pub fn get_numeric_circuit_id(circuit_wrapper: &CircuitWrapper) -> u8 {
    match circuit_wrapper {
        CircuitWrapper::Base(circuit) => circuit.numeric_circuit_type(),
        CircuitWrapper::Recursive(circuit) => circuit.numeric_circuit_type(),
    }
}
