use std::time::Instant;

use zksync_object_store::ObjectStore;
use zksync_prover_dal::{Connection, Prover, ProverDal};
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::gadgets::queue::full_state_queue::FullStateCircuitQueueRawWitness,
        circuit_definitions::{
            base_layer::ZkSyncBaseLayerCircuit,
            recursion_layer::{
                base_circuit_type_into_recursive_leaf_circuit_type, ZkSyncRecursionLayerStorageType,
            },
        },
        zkevm_circuits::scheduler::aux::BaseLayerCircuitType,
    },
    get_current_pod_name,
    keys::{FriCircuitKey, RamPermutationQueueWitnessKey},
    CircuitWrapper, ProverJob, ProverServiceDataKey, RamPermutationQueueWitness,
};
use zksync_types::{
    basic_fri_types::{AggregationRound, CircuitIdRoundTuple},
    protocol_version::ProtocolSemanticVersion,
};

use crate::metrics::{CircuitLabels, PROVER_FRI_UTILS_METRICS};

pub mod metrics;
pub mod region_fetcher;
pub mod socket_utils;

pub async fn fetch_next_circuit(
    storage: &mut Connection<'_, Prover>,
    blob_store: &dyn ObjectStore,
    circuit_ids_for_round_to_be_proven: &[CircuitIdRoundTuple],
    protocol_version: &ProtocolSemanticVersion,
) -> Option<ProverJob> {
    let pod_name = get_current_pod_name();
    let prover_job = match &circuit_ids_for_round_to_be_proven.is_empty() {
        false => {
            // Specialized prover: proving subset of configured circuits.
            storage
                .fri_prover_jobs_dal()
                .get_next_job_for_circuit_id_round(
                    circuit_ids_for_round_to_be_proven,
                    *protocol_version,
                    &pod_name,
                )
                .await
        }
        true => {
            // Generalized prover: proving all circuits.
            storage
                .fri_prover_jobs_dal()
                .get_next_job(*protocol_version, &pod_name)
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
    let circuit_wrapper = blob_store
        .get(circuit_key)
        .await
        .unwrap_or_else(|err| panic!("{err:?}"));
    let input = match circuit_wrapper {
        a @ CircuitWrapper::Base(_) => a,
        a @ CircuitWrapper::Recursive(_) => a,
        CircuitWrapper::BaseWithAuxData((circuit, aux_data)) => {
            // inject additional data
            if let ZkSyncBaseLayerCircuit::RAMPermutation(circuit_instance) = circuit {
                let sorted_witness_key = RamPermutationQueueWitnessKey {
                    block_number: prover_job.block_number,
                    circuit_subsequence_number: aux_data.circuit_subsequence_number as usize,
                    is_sorted: true,
                };

                let sorted_witness_handle = blob_store.get(sorted_witness_key);

                let unsorted_witness_key = RamPermutationQueueWitnessKey {
                    block_number: prover_job.block_number,
                    circuit_subsequence_number: aux_data.circuit_subsequence_number as usize,
                    is_sorted: false,
                };

                let unsorted_witness_handle = blob_store.get(unsorted_witness_key);

                let unsorted_witness: RamPermutationQueueWitness =
                    unsorted_witness_handle.await.unwrap();
                let sorted_witness: RamPermutationQueueWitness =
                    sorted_witness_handle.await.unwrap();

                let mut witness = circuit_instance.witness.take().unwrap();
                witness.unsorted_queue_witness = FullStateCircuitQueueRawWitness {
                    elements: unsorted_witness.witness.into(),
                };
                witness.sorted_queue_witness = FullStateCircuitQueueRawWitness {
                    elements: sorted_witness.witness.into(),
                };
                circuit_instance.witness.store(Some(witness));

                CircuitWrapper::Base(ZkSyncBaseLayerCircuit::RAMPermutation(circuit_instance))
            } else {
                panic!("Unexpected circuit received with aux data");
            }
        }
        _ => panic!("Invalid circuit wrapper received"),
    };

    let label = CircuitLabels {
        circuit_type: prover_job.circuit_id,
        aggregation_round: prover_job.aggregation_round.into(),
    };
    PROVER_FRI_UTILS_METRICS.blob_fetch_time[&label].observe(started_at.elapsed());

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
        CircuitWrapper::Base(circuit) | CircuitWrapper::BaseWithAuxData((circuit, _)) => {
            circuit.numeric_circuit_type()
        }
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
    ZkSyncRecursionLayerStorageType::leafs_as_iter_u8()
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
                ..=(ZkSyncRecursionLayerStorageType::LeafLayerCircuitForEIP4844Repack as u8))
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
