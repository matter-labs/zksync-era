#![feature(generic_const_exprs)]

use crate::in_memory_setup_data_source::InMemoryDataSource;
use zkevm_test_harness::compute_setups::{
    generate_base_layer_vks_and_proofs, generate_recursive_layer_vks_and_proofs,
};
use zkevm_test_harness::data_source::SetupDataSource;
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerStorageType;
use zksync_prover_fri_types::circuit_definitions::zkevm_circuits::scheduler::aux::BaseLayerCircuitType;
use zksync_prover_fri_types::ProverServiceDataKey;
use zksync_types::proofs::AggregationRound;
use zksync_vk_setup_data_server_fri::{
    get_round_for_recursive_circuit_type, save_base_layer_vk, save_finalization_hints,
    save_recursive_layer_vk,
};

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

fn save_finalization_hints_using_source(source: &dyn SetupDataSource) {
    for base_circuit_type in
        (BaseLayerCircuitType::VM as u8)..=(BaseLayerCircuitType::L1MessagesHasher as u8)
    {
        let hint = source
            .get_base_layer_finalization_hint(base_circuit_type)
            .unwrap_or_else(|_| {
                panic!(
                    "No finalization_hint exist for circuit type: {}",
                    base_circuit_type
                )
            })
            .into_inner();
        let key = ProverServiceDataKey::new(base_circuit_type, AggregationRound::BasicCircuits);
        save_finalization_hints(key, &hint);
    }
    for leaf_circuit_type in (ZkSyncRecursionLayerStorageType::LeafLayerCircuitForMainVM as u8)
        ..=(ZkSyncRecursionLayerStorageType::LeafLayerCircuitForL1MessagesHasher as u8)
    {
        let hint = source
            .get_recursion_layer_finalization_hint(leaf_circuit_type)
            .unwrap_or_else(|_| {
                panic!(
                    "No finalization hint exist for circuit type: {}",
                    leaf_circuit_type
                )
            })
            .into_inner();
        let key = ProverServiceDataKey::new(
            leaf_circuit_type,
            get_round_for_recursive_circuit_type(leaf_circuit_type),
        );
        save_finalization_hints(key, &hint);
    }

    let node_hint = source
        .get_recursion_layer_node_finalization_hint()
        .expect("No finalization hint exist for node layer circuit")
        .into_inner();
    save_finalization_hints(
        ProverServiceDataKey::new(
            ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
            AggregationRound::NodeAggregation,
        ),
        &node_hint,
    );

    let scheduler_hint = source
        .get_recursion_layer_finalization_hint(
            ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
        )
        .expect("No finalization hint exist for scheduler layer circuit")
        .into_inner();
    save_finalization_hints(
        ProverServiceDataKey::new(
            ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
            AggregationRound::Scheduler,
        ),
        &scheduler_hint,
    );
}

fn generate_vks() {
    let mut in_memory_source = InMemoryDataSource::new();
    generate_base_layer_vks_and_proofs(&mut in_memory_source).expect("Failed generating base vk's");
    generate_recursive_layer_vks_and_proofs(&mut in_memory_source)
        .expect("Failed generating recursive vk's");
    save_finalization_hints_using_source(&in_memory_source);
    save_vks(&in_memory_source);
}

fn main() {
    generate_vks();
}
