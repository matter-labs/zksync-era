use zksync_prover_fri_types::circuit_definitions::{
    circuit_definitions::recursion_layer::base_circuit_type_into_recursive_leaf_circuit_type,
    zkevm_circuits::scheduler::aux::BaseLayerCircuitType,
};

pub mod metrics;

// TODO: This should be moved into harness, once done the entire module can be removed.
// Note, metrics.rs can be moved to WG.
pub fn get_recursive_layer_circuit_id_for_base_layer(base_layer_circuit_id: u8) -> u8 {
    let recursive_circuit_type = base_circuit_type_into_recursive_leaf_circuit_type(
        BaseLayerCircuitType::from_numeric_value(base_layer_circuit_id),
    );
    recursive_circuit_type as u8
}
