#![feature(generic_const_exprs)]

use crate::in_memory_setup_data_source::InMemoryDataSource;
use circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerStorageType;
use circuit_definitions::zkevm_circuits::scheduler::aux::BaseLayerCircuitType;
use zkevm_test_harness::compute_setups::{
    generate_base_layer_vks_and_proofs, generate_recursive_layer_vks_and_proofs,
};
use zkevm_test_harness::data_source::SetupDataSource;
use zksync_vk_setup_data_server_fri::{save_base_layer_vk, save_recursive_layer_vk};

mod in_memory_setup_data_source;
mod tests;
mod vk_generator;

fn save_vks(source: &dyn SetupDataSource) {
    for base_circuit_type in
        (BaseLayerCircuitType::VM as u8)..=(BaseLayerCircuitType::L1MessagesHasher as u8)
    {
        let vk = source
            .get_base_layer_vk(base_circuit_type)
            .unwrap_or_else(|_| panic!("No vk exist for circuit type: {}", base_circuit_type));
        save_base_layer_vk(vk);
    }
    for leaf_circuit_type in (ZkSyncRecursionLayerStorageType::LeafLayerCircuitForMainVM as u8)
        ..=(ZkSyncRecursionLayerStorageType::LeafLayerCircuitForL1MessagesHasher as u8)
    {
        let vk = source
            .get_recursion_layer_vk(leaf_circuit_type)
            .unwrap_or_else(|_| panic!("No vk exist for circuit type: {}", leaf_circuit_type));
        save_recursive_layer_vk(vk);
    }
    save_recursive_layer_vk(
        source
            .get_recursion_layer_node_vk()
            .expect("No vk exist for node layer circuit"),
    );
    save_recursive_layer_vk(
        source
            .get_recursion_layer_vk(ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8)
            .expect("No vk exist for scheduler circuit"),
    );
}

fn generate_vks() {
    let mut in_memory_source = InMemoryDataSource::new();
    generate_base_layer_vks_and_proofs(&mut in_memory_source).expect("Failed generating base vk's");
    generate_recursive_layer_vks_and_proofs(&mut in_memory_source)
        .expect("Failed generating recursive vk's");
    save_vks(&in_memory_source);
}

fn main() {
    generate_vks();
}
