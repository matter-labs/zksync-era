use anyhow::Context as _;
use zkevm_test_harness::witness::recursive_aggregation::compute_leaf_params;
use zksync_prover_fri_types::circuit_definitions::{
    boojum::field::goldilocks::GoldilocksField,
    circuit_definitions::recursion_layer::base_circuit_type_into_recursive_leaf_circuit_type,
    zkevm_circuits::{
        recursion::leaf_layer::input::RecursionLeafParametersWitness,
        scheduler::aux::BaseLayerCircuitType,
    },
};

use crate::keystore::Keystore;

pub fn get_leaf_vk_params(
    keystore: &Keystore,
) -> anyhow::Result<Vec<(u8, RecursionLeafParametersWitness<GoldilocksField>)>> {
    let mut leaf_vk_commits = vec![];

    for circuit_type in
        (BaseLayerCircuitType::VM as u8)..=(BaseLayerCircuitType::L1MessagesHasher as u8)
    {
        let recursive_circuit_type = base_circuit_type_into_recursive_leaf_circuit_type(
            BaseLayerCircuitType::from_numeric_value(circuit_type),
        );
        let base_vk = keystore
            .load_base_layer_verification_key(circuit_type)
            .with_context(|| format!("get_base_layer_vk_for_circuit_type({circuit_type})"))?;
        let leaf_vk = keystore
            .load_recursive_layer_verification_key(recursive_circuit_type as u8)
            .with_context(|| {
                format!("get_recursive_layer_vk_for_circuit_type({recursive_circuit_type:?})")
            })?;
        let params = compute_leaf_params(circuit_type, base_vk, leaf_vk);
        leaf_vk_commits.push((circuit_type, params));
    }
    Ok(leaf_vk_commits)
}
