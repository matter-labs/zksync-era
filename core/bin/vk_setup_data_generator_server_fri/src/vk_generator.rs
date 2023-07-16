use circuit_definitions::boojum::worker::Worker;
use circuit_definitions::circuit_definitions::base_layer::ZkSyncBaseLayerVerificationKey;
use circuit_definitions::{BASE_LAYER_CAP_SIZE, BASE_LAYER_FRI_LDE_FACTOR};
use zkevm_test_harness::geometry_config::get_geometry_config;
use zkevm_test_harness::prover_utils::{
    create_base_layer_setup_data, create_recursive_layer_setup_data,
};
use zksync_vk_setup_data_server_fri::utils::{get_basic_circuits, get_leaf_circuits, CYCLE_LIMIT};
use zksync_vk_setup_data_server_fri::{save_base_layer_vk, save_recursive_layer_vk};

use circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerVerificationKey;

fn main() {
    vlog::info!("starting vk generator");
    generate_basic_circuit_vks();
}

pub fn generate_basic_circuit_vks() {
    let worker = Worker::new();
    for circuit in get_basic_circuits(CYCLE_LIMIT, get_geometry_config()) {
        let circuit_type = circuit.numeric_circuit_type();
        let (_, _, vk, _, _, _, _) = create_base_layer_setup_data(
            circuit.clone(),
            &worker,
            BASE_LAYER_FRI_LDE_FACTOR,
            BASE_LAYER_CAP_SIZE,
        );
        let typed_vk = ZkSyncBaseLayerVerificationKey::from_inner(circuit_type, vk);
        save_base_layer_vk(typed_vk);
    }
}

pub fn generate_leaf_layer_vks() {
    let worker = Worker::new();
    for circuit in get_leaf_circuits() {
        let circuit_type = circuit.numeric_circuit_type();
        let (_setup_base, _setup, vk, _setup_tree, _vars_hint, _wits_hint, _finalization_hint) =
            create_recursive_layer_setup_data(
                circuit.clone(),
                &worker,
                BASE_LAYER_FRI_LDE_FACTOR,
                BASE_LAYER_CAP_SIZE,
            );

        let typed_vk = ZkSyncRecursionLayerVerificationKey::from_inner(circuit_type, vk.clone());
        save_recursive_layer_vk(typed_vk);
    }
}
