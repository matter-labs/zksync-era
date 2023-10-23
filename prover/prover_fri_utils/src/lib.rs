use std::time::Instant;

use zksync_dal::StorageProcessor;
use zksync_object_store::{FriCircuitKey, ObjectStore};
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::base_circuit_type_into_recursive_leaf_circuit_type;
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerStorageType;
use zksync_prover_fri_types::circuit_definitions::zkevm_circuits::scheduler::aux::BaseLayerCircuitType;
use zksync_types::basic_fri_types::CircuitIdRoundTuple;

use zksync_prover_fri_types::{
    get_current_pod_name, CircuitWrapper, ProverJob, ProverServiceDataKey,
};

use zksync_types::proofs::AggregationRound;
use zksync_types::protocol_version::L1VerifierConfig;

pub mod socket_utils;

pub async fn fetch_next_circuit(
    storage: &mut StorageProcessor<'_>,
    blob_store: &dyn ObjectStore,
    circuit_ids_for_round_to_be_proven: &Vec<CircuitIdRoundTuple>,
    vk_commitments: &L1VerifierConfig,
) -> Option<ProverJob> {
    let protocol_versions = storage
        .fri_protocol_versions_dal()
        .protocol_version_for(vk_commitments)
        .await;
    let pod_name = get_current_pod_name();
    let prover_job = match &circuit_ids_for_round_to_be_proven.is_empty() {
        false => {
            // Specialized prover: proving subset of configured circuits.
            storage
                .fri_prover_jobs_dal()
                .get_next_job_for_circuit_id_round(
                    circuit_ids_for_round_to_be_proven,
                    &protocol_versions,
                    &pod_name,
                )
                .await
        }
        true => {
            // Generalized prover: proving all circuits.
            storage
                .fri_prover_jobs_dal()
                .get_next_job(&protocol_versions, &pod_name)
                .await
        }
    }?;
    tracing::info!("Started processing prover job: {:?}", prover_job);

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

pub fn get_all_circuit_id_round_tuples_for(
    ids: Vec<CircuitIdRoundTuple>,
) -> Vec<CircuitIdRoundTuple> {
    ids.into_iter()
        .flat_map(|id_round_tuple| {
            if id_round_tuple.aggregation_round == AggregationRound::NodeAggregation as u8 {
                get_all_circuit_id_round_tuples_for_node_aggregation()
            } else {
                vec![id_round_tuple]
            }
        })
        .collect()
}

fn get_all_circuit_id_round_tuples_for_node_aggregation() -> Vec<CircuitIdRoundTuple> {
    ((ZkSyncRecursionLayerStorageType::LeafLayerCircuitForMainVM as u8)
        ..=(ZkSyncRecursionLayerStorageType::LeafLayerCircuitForL1MessagesHasher as u8))
        .map(|circuit_id| CircuitIdRoundTuple {
            circuit_id,
            aggregation_round: AggregationRound::NodeAggregation as u8,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_all_circuit_id_round_tuples_with_node_aggregation() {
        let ids = vec![
            CircuitIdRoundTuple {
                circuit_id: ZkSyncRecursionLayerStorageType::LeafLayerCircuitForMainVM as u8,
                aggregation_round: AggregationRound::NodeAggregation as u8,
            },
            CircuitIdRoundTuple {
                circuit_id: ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
                aggregation_round: AggregationRound::Scheduler as u8,
            },
        ];
        let res = get_all_circuit_id_round_tuples_for(ids);
        let expected_circuit_ids: Vec<u8> =
            ((ZkSyncRecursionLayerStorageType::LeafLayerCircuitForMainVM as u8)
                ..=(ZkSyncRecursionLayerStorageType::LeafLayerCircuitForL1MessagesHasher as u8))
                .collect();
        let expected = expected_circuit_ids
            .into_iter()
            .map(|circuit_id| CircuitIdRoundTuple {
                circuit_id,
                aggregation_round: AggregationRound::NodeAggregation as u8,
            })
            .chain(std::iter::once(CircuitIdRoundTuple {
                circuit_id: ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
                aggregation_round: AggregationRound::Scheduler as u8,
            }))
            .collect::<Vec<_>>();

        assert_eq!(expected, res);
    }

    #[test]
    fn test_get_all_circuit_id_round_tuples_for_without_node_aggregation() {
        let ids = vec![
            CircuitIdRoundTuple {
                circuit_id: 7,
                aggregation_round: 1,
            },
            CircuitIdRoundTuple {
                circuit_id: 8,
                aggregation_round: 1,
            },
            CircuitIdRoundTuple {
                circuit_id: 10,
                aggregation_round: 1,
            },
            CircuitIdRoundTuple {
                circuit_id: 11,
                aggregation_round: 1,
            },
        ];

        let res = get_all_circuit_id_round_tuples_for(ids.clone());
        assert_eq!(ids, res);
    }
}
