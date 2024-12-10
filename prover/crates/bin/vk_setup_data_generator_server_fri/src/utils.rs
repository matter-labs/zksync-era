use circuit_definitions::{
    boojum::worker::Worker,
    circuit_definitions::{
        aux_layer::{
            ZkSyncCompressionForWrapperCircuit, ZkSyncCompressionLayerCircuit,
            ZkSyncCompressionLayerStorage, ZkSyncSnarkWrapperSetup, ZkSyncSnarkWrapperVK,
        },
        recursion_layer::{ZkSyncRecursionLayerStorageType, ZkSyncRecursionLayerVerificationKey},
    },
};
#[cfg(feature = "gpu")]
use shivini::cs::gpu_setup_and_vk_from_base_setup_vk_params_and_hints;
use zkevm_test_harness::{
    data_source::{BlockDataSource, SetupDataSource},
    proof_wrapper_utils::{
        check_trusted_setup_file_existace, get_vk_for_previous_circuit,
        get_wrapper_setup_and_vk_from_compression_vk, WrapperConfig,
    },
    prover_utils::light::{
        create_light_compression_for_wrapper_setup_data, create_light_compression_layer_setup_data,
    },
};

#[cfg(feature = "gpu")]
pub(crate) fn generate_compression_vks<DS: SetupDataSource + BlockDataSource>(
    config: WrapperConfig,
    source: &mut DS,
    worker: &Worker,
) {
    for circuit_type in config.get_compression_types() {
        let vk = get_vk_for_previous_circuit(source, circuit_type).unwrap_or_else(|_| {
            panic!(
                "VK of previous circuit should be present. Current circuit type: {}",
                circuit_type
            )
        });

        let compression_circuit =
            ZkSyncCompressionLayerCircuit::from_witness_and_vk(None, vk, circuit_type);
        let proof_config = compression_circuit.proof_config_for_compression_step();

        let (setup_base, vk_geometry, vars_hint, witness_hint, finalization_hint) =
            create_light_compression_layer_setup_data(
                compression_circuit,
                worker,
                proof_config.fri_lde_factor,
                proof_config.merkle_tree_cap_size,
            );

        let (_, vk) = gpu_setup_and_vk_from_base_setup_vk_params_and_hints(
            setup_base,
            vk_geometry,
            vars_hint.clone(),
            witness_hint,
            worker,
        )
        .expect("failed creating GPU compression layer setup data");

        source
            .set_compression_vk(ZkSyncCompressionLayerStorage::from_inner(
                circuit_type,
                vk.clone(),
            ))
            .unwrap();
        source
            .set_compression_hint(ZkSyncCompressionLayerStorage::from_inner(
                circuit_type,
                finalization_hint.clone(),
            ))
            .unwrap();
    }
}

#[cfg(feature = "gpu")]
pub(crate) fn generate_compression_for_wrapper_vks<DS: SetupDataSource + BlockDataSource>(
    config: WrapperConfig,
    source: &mut DS,
    worker: &Worker,
) {
    let compression_for_wrapper_type = config.get_compression_for_wrapper_type();
    let vk = get_vk_for_previous_circuit(source, compression_for_wrapper_type).unwrap();

    let circuit = ZkSyncCompressionForWrapperCircuit::from_witness_and_vk(
        None,
        vk,
        compression_for_wrapper_type,
    );

    let proof_config = circuit.proof_config_for_compression_step();

    let (setup_base, vk_geometry, vars_hint, witness_hint, finalization_hint) =
        create_light_compression_for_wrapper_setup_data(
            circuit,
            worker,
            proof_config.fri_lde_factor,
            proof_config.merkle_tree_cap_size,
        );

    let (_, vk) = gpu_setup_and_vk_from_base_setup_vk_params_and_hints(
        setup_base,
        vk_geometry,
        vars_hint.clone(),
        witness_hint,
        worker,
    )
    .expect("failed creating GPU compression for wrapper layer setup data");

    source
        .set_compression_for_wrapper_vk(ZkSyncCompressionLayerStorage::from_inner(
            compression_for_wrapper_type,
            vk.clone(),
        ))
        .unwrap();
    source
        .set_compression_for_wrapper_hint(ZkSyncCompressionLayerStorage::from_inner(
            compression_for_wrapper_type,
            finalization_hint.clone(),
        ))
        .unwrap();
}

/// Computes wrapper vk from scheduler vk
/// We store all vks in the RAM
pub fn get_plonk_wrapper_setup_and_vk_from_scheduler_vk<DS: SetupDataSource + BlockDataSource>(
    source: &mut DS,
    vk: ZkSyncRecursionLayerVerificationKey,
    config: WrapperConfig,
) -> (ZkSyncSnarkWrapperSetup, ZkSyncSnarkWrapperVK) {
    // Check trusted setup file for later
    check_trusted_setup_file_existace();

    // Check circuit type correctness
    assert_eq!(
        vk.numeric_circuit_type(),
        ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8
    );

    let wrapper_type = config.get_wrapper_type();
    let wrapper_vk = source.get_compression_for_wrapper_vk(wrapper_type).unwrap();
    get_wrapper_setup_and_vk_from_compression_vk(wrapper_vk, config)
}
