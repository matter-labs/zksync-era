use anyhow::Context as _;
use zkevm_test_harness::{
    geometry_config::get_geometry_config,
    prover_utils::{create_base_layer_setup_data, create_recursive_layer_setup_data},
};
use zksync_prover_fri_types::{
    circuit_definitions::{
        boojum::worker::Worker,
        circuit_definitions::{
            base_layer::ZkSyncBaseLayerVerificationKey,
            recursion_layer::ZkSyncRecursionLayerVerificationKey,
        },
        BASE_LAYER_CAP_SIZE, BASE_LAYER_FRI_LDE_FACTOR,
    },
    ProverServiceDataKey,
};
use zksync_types::basic_fri_types::AggregationRound;
use zksync_vk_setup_data_server_fri::{
    get_round_for_recursive_circuit_type, save_base_layer_vk, save_finalization_hints,
    save_recursive_layer_vk,
    utils::{get_basic_circuits, get_leaf_circuits, CYCLE_LIMIT},
};

#[allow(dead_code)]
fn main() -> anyhow::Result<()> {
    tracing::info!("starting vk generator");
    generate_basic_circuit_vks().context("generate_basic_circuit_vks()")
}

pub fn generate_basic_circuit_vks() -> anyhow::Result<()> {
    let worker = Worker::new();
    for circuit in
        get_basic_circuits(CYCLE_LIMIT, get_geometry_config()).context("get_basic_circuits()")?
    {
        let circuit_type = circuit.numeric_circuit_type();
        let (_, _, vk, _, _, _, finalization_hint) = create_base_layer_setup_data(
            circuit.clone(),
            &worker,
            BASE_LAYER_FRI_LDE_FACTOR,
            BASE_LAYER_CAP_SIZE,
        );
        let typed_vk = ZkSyncBaseLayerVerificationKey::from_inner(circuit_type, vk);
        save_base_layer_vk(typed_vk).context("save_base_layer_vk()")?;
        let key = ProverServiceDataKey::new(circuit_type, AggregationRound::BasicCircuits);
        save_finalization_hints(key, &finalization_hint).context("save_finalization_hints()")?;
    }
    Ok(())
}

#[allow(dead_code)]
pub fn generate_leaf_layer_vks() -> anyhow::Result<()> {
    let worker = Worker::new();
    for circuit in get_leaf_circuits().context("get_leaf_circuits()")? {
        let circuit_type = circuit.numeric_circuit_type();
        let (_setup_base, _setup, vk, _setup_tree, _vars_hint, _wits_hint, finalization_hint) =
            create_recursive_layer_setup_data(
                circuit.clone(),
                &worker,
                BASE_LAYER_FRI_LDE_FACTOR,
                BASE_LAYER_CAP_SIZE,
            );

        let typed_vk = ZkSyncRecursionLayerVerificationKey::from_inner(circuit_type, vk.clone());
        save_recursive_layer_vk(typed_vk).context("save_recursive_layer_vk()")?;
        let key = ProverServiceDataKey::new(
            circuit_type,
            get_round_for_recursive_circuit_type(circuit_type),
        );
        save_finalization_hints(key, &finalization_hint).context("save_finalization_hints()")?;
    }
    Ok(())
}
