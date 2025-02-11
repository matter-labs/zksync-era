use proptest::prelude::*;
use zksync_prover_fri_types::{
    circuit_definitions::{
        circuit_definitions::recursion_layer::base_circuit_type_into_recursive_leaf_circuit_type,
        zkevm_circuits::scheduler::aux::BaseLayerCircuitType,
    },
    ProverServiceDataKey, ProvingStage,
};
use zksync_prover_keystore::keystore::Keystore;

fn all_possible_prover_service_data_key() -> impl Strategy<Value = ProverServiceDataKey> {
    let mut keys = Vec::with_capacity(30);
    for circuit_type in 1..=13 {
        keys.push(ProverServiceDataKey::new(
            circuit_type,
            ProvingStage::BasicCircuits,
        ));
        let recursive_circuit_type = base_circuit_type_into_recursive_leaf_circuit_type(
            BaseLayerCircuitType::from_numeric_value(circuit_type),
        ) as u8;
        keys.push(ProverServiceDataKey::new(
            recursive_circuit_type,
            ProvingStage::LeafAggregation,
        ));
    }
    keys.push(ProverServiceDataKey::new(1, ProvingStage::Scheduler));
    keys.push(ProverServiceDataKey::new(2, ProvingStage::NodeAggregation));

    prop::sample::select(keys)
}

proptest! {
    #[test]
    fn test_get_base_layer_vk_for_circuit_type(circuit_id in 1u8..13) {
        let keystore = Keystore::locate();
        let vk = keystore.load_base_layer_verification_key(circuit_id).unwrap();
        assert_eq!(circuit_id, vk.numeric_circuit_type());
    }

    #[test]
    fn test_get_recursive_layer_vk_for_circuit_type(circuit_id in 1u8..15) {
        let keystore = Keystore::locate();
        let vk = keystore.load_recursive_layer_verification_key(circuit_id).unwrap();
        assert_eq!(circuit_id, vk.numeric_circuit_type());
    }

    #[test]
    fn test_get_finalization_hints(key in all_possible_prover_service_data_key()) {
        let keystore = Keystore::locate();

        let result = keystore.load_finalization_hints(key).unwrap();

        assert!(!result.row_finalization_hints.is_empty(), "Row finalization hints should not be empty");
        assert!(!result.public_inputs.is_empty(), "Public inputs should not be empty");

        assert!(result.nop_gates_to_add > 0, "Nop gates to add should be more than 0");
        assert!(result.final_trace_len > 0, "Final trace length should be more than 0");
    }

}

// Test `ProverServiceDataKey::new` method
#[test]
fn test_proverservicedatakey_new() {
    let key = ProverServiceDataKey::new(1, ProvingStage::BasicCircuits);
    assert_eq!(
        key.circuit_id, 1,
        "Circuit id should be equal to the given value"
    );
    assert_eq!(
        key.stage,
        ProvingStage::BasicCircuits,
        "Round should be equal to the given value"
    );
}
