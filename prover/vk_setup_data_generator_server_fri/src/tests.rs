use proptest::prelude::*;
use zksync_prover_fri_types::{
    circuit_definitions::{
        circuit_definitions::recursion_layer::{
            base_circuit_type_into_recursive_leaf_circuit_type, ZkSyncRecursionLayerStorageType,
        },
        zkevm_circuits::scheduler::aux::BaseLayerCircuitType,
    },
    ProverServiceDataKey,
};
use zksync_types::basic_fri_types::AggregationRound;
use zksync_vk_setup_data_server_fri::{
    get_base_layer_vk_for_circuit_type, get_base_path, get_file_path, get_finalization_hints,
    get_recursive_layer_vk_for_circuit_type, get_round_for_recursive_circuit_type,
    ProverServiceDataType,
};

fn all_possible_prover_service_data_key() -> impl Strategy<Value = ProverServiceDataKey> {
    let mut keys = Vec::with_capacity(30);
    for circuit_type in 1..=13 {
        keys.push(ProverServiceDataKey::new(
            circuit_type,
            AggregationRound::BasicCircuits,
        ));
        let recursive_circuit_type = base_circuit_type_into_recursive_leaf_circuit_type(
            BaseLayerCircuitType::from_numeric_value(circuit_type),
        ) as u8;
        keys.push(ProverServiceDataKey::new(
            recursive_circuit_type,
            AggregationRound::LeafAggregation,
        ));
    }
    keys.push(ProverServiceDataKey::new(1, AggregationRound::Scheduler));
    keys.push(ProverServiceDataKey::new(
        2,
        AggregationRound::NodeAggregation,
    ));

    prop::sample::select(keys)
}

proptest! {
    #[test]
    fn test_get_base_layer_vk_for_circuit_type(circuit_id in 1u8..13) {
        let vk = get_base_layer_vk_for_circuit_type(circuit_id).unwrap();
        assert_eq!(circuit_id, vk.numeric_circuit_type());
    }

    #[test]
    fn test_get_recursive_layer_vk_for_circuit_type(circuit_id in 1u8..15) {
        let vk = get_recursive_layer_vk_for_circuit_type(circuit_id).unwrap();
        assert_eq!(circuit_id, vk.numeric_circuit_type());
    }

    #[test]
    fn test_get_finalization_hints(key in all_possible_prover_service_data_key()) {
        let result = get_finalization_hints(key).unwrap();

        assert!(!result.row_finalization_hints.is_empty(), "Row finalization hints should not be empty");
        assert!(!result.public_inputs.is_empty(), "Public inputs should not be empty");

        assert!(result.nop_gates_to_add > 0, "Nop gates to add should be more than 0");
        assert!(result.final_trace_len > 0, "Final trace length should be more than 0");
    }

}

// Test `get_base_path` method
#[test]
fn test_get_base_path() {
    let base_path = get_base_path();
    assert!(!base_path.is_empty(), "Base path should not be empty");
}

// Test `get_file_path` method
#[test]
fn test_get_file_path() {
    let key = ProverServiceDataKey::new(1, AggregationRound::BasicCircuits);
    let file_path = get_file_path(key, ProverServiceDataType::VerificationKey).unwrap();
    assert!(!file_path.is_empty(), "File path should not be empty");
}

// Test `ProverServiceDataKey::new` method
#[test]
fn test_proverservicedatakey_new() {
    let key = ProverServiceDataKey::new(1, AggregationRound::BasicCircuits);
    assert_eq!(
        key.circuit_id, 1,
        "Circuit id should be equal to the given value"
    );
    assert_eq!(
        key.round,
        AggregationRound::BasicCircuits,
        "Round should be equal to the given value"
    );
}

// Test `get_round_for_recursive_circuit_type` method
#[test]
fn test_get_round_for_recursive_circuit_type() {
    let round = get_round_for_recursive_circuit_type(
        ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
    );
    assert_eq!(
        round,
        AggregationRound::Scheduler,
        "Round should be scheduler"
    );
}
