use anyhow::Context as _;
use std::sync::Arc;

use circuit_definitions::{
    aux_definitions::witness_oracle::VmWitnessOracle,
    boojum::{
        cs::traits::circuit::CircuitBuilder, field::goldilocks::GoldilocksField,
        implementations::poseidon2::Poseidon2Goldilocks,
    },
    circuit_definitions::{
        base_layer::{StorageApplicationInstanceSynthesisFunction, ZkSyncBaseLayerCircuit},
        ZkSyncUniformCircuitInstance,
    },
    zkevm_circuits::{
        code_unpacker_sha256::input::CodeDecommitterCircuitInstanceWitness,
        demux_log_queue::input::LogDemuxerCircuitInstanceWitness,
        ecrecover::EcrecoverCircuitInstanceWitness,
        fsm_input_output::circuit_inputs::main_vm::VmCircuitWitness,
        keccak256_round_function::input::Keccak256RoundFunctionCircuitInstanceWitness,
        linear_hasher::input::LinearHasherCircuitInstanceWitness,
        log_sorter::input::EventsDeduplicatorInstanceWitness,
        ram_permutation::input::RamPermutationCircuitInstanceWitness,
        sha256_round_function::input::Sha256RoundFunctionCircuitInstanceWitness,
        sort_decommittment_requests::input::CodeDecommittmentsDeduplicatorInstanceWitness,
        storage_application::input::StorageApplicationCircuitInstanceWitness,
        storage_validity_by_grand_product::input::StorageDeduplicatorInstanceWitness,
    },
};
use crossbeam::atomic::AtomicCell;
use zkevm_test_harness::{
    geometry_config::get_geometry_config,
    prover_utils::{create_base_layer_setup_data, create_recursive_layer_setup_data},
    toolset::GeometryConfig,
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

fn get_all_basic_circuits(
    geometry: &GeometryConfig,
) -> Vec<
    ZkSyncBaseLayerCircuit<GoldilocksField, VmWitnessOracle<GoldilocksField>, Poseidon2Goldilocks>,
> {
    vec![
        ZkSyncBaseLayerCircuit::MainVM(ZkSyncUniformCircuitInstance {
            witness: AtomicCell::new(Some(VmCircuitWitness::<
                GoldilocksField,
                VmWitnessOracle<GoldilocksField>,
            >::default())),
            config: Arc::new(geometry.cycles_per_vm_snapshot as usize),
            round_function: Arc::new(Poseidon2Goldilocks),
            expected_public_input: None,
        }),
        ZkSyncBaseLayerCircuit::CodeDecommittmentsSorter(ZkSyncUniformCircuitInstance {
            witness: AtomicCell::new(Some(CodeDecommittmentsDeduplicatorInstanceWitness::<
                GoldilocksField,
            >::default())),
            config: Arc::new(geometry.cycles_code_decommitter_sorter as usize),
            round_function: Arc::new(Poseidon2Goldilocks),
            expected_public_input: None,
        }),
        ZkSyncBaseLayerCircuit::CodeDecommitter(ZkSyncUniformCircuitInstance {
            witness: AtomicCell::new(Some(
                CodeDecommitterCircuitInstanceWitness::<GoldilocksField>::default(),
            )),
            config: Arc::new(geometry.cycles_per_code_decommitter as usize),
            round_function: Arc::new(Poseidon2Goldilocks),
            expected_public_input: None,
        }),
        ZkSyncBaseLayerCircuit::LogDemuxer(ZkSyncUniformCircuitInstance {
            witness: AtomicCell::new(Some(
                LogDemuxerCircuitInstanceWitness::<GoldilocksField>::default(),
            )),
            config: Arc::new(geometry.cycles_per_log_demuxer as usize),
            round_function: Arc::new(Poseidon2Goldilocks),
            expected_public_input: None,
        }),
        ZkSyncBaseLayerCircuit::KeccakRoundFunction(ZkSyncUniformCircuitInstance {
            witness: AtomicCell::new(Some(Keccak256RoundFunctionCircuitInstanceWitness::<
                GoldilocksField,
            >::default())),
            config: Arc::new(geometry.cycles_per_keccak256_circuit as usize),
            round_function: Arc::new(Poseidon2Goldilocks),
            expected_public_input: None,
        }),
        ZkSyncBaseLayerCircuit::Sha256RoundFunction(ZkSyncUniformCircuitInstance {
            witness: AtomicCell::new(Some(Sha256RoundFunctionCircuitInstanceWitness::<
                GoldilocksField,
            >::default())),
            config: Arc::new(geometry.cycles_per_sha256_circuit as usize),
            round_function: Arc::new(Poseidon2Goldilocks),
            expected_public_input: None,
        }),
        ZkSyncBaseLayerCircuit::ECRecover(ZkSyncUniformCircuitInstance {
            witness: AtomicCell::new(Some(
                EcrecoverCircuitInstanceWitness::<GoldilocksField>::default(),
            )),
            config: Arc::new(geometry.cycles_per_ecrecover_circuit as usize),
            round_function: Arc::new(Poseidon2Goldilocks),
            expected_public_input: None,
        }),
        ZkSyncBaseLayerCircuit::RAMPermutation(ZkSyncUniformCircuitInstance {
            witness: AtomicCell::new(Some(
                RamPermutationCircuitInstanceWitness::<GoldilocksField>::default(),
            )),
            config: Arc::new(geometry.cycles_per_ram_permutation as usize),
            round_function: Arc::new(Poseidon2Goldilocks),
            expected_public_input: None,
        }),
        ZkSyncBaseLayerCircuit::StorageSorter(ZkSyncUniformCircuitInstance {
            witness: AtomicCell::new(Some(
                StorageDeduplicatorInstanceWitness::<GoldilocksField>::default(),
            )),
            config: Arc::new(geometry.cycles_per_storage_sorter as usize),
            round_function: Arc::new(Poseidon2Goldilocks),
            expected_public_input: None,
        }),
        ZkSyncBaseLayerCircuit::StorageApplication(ZkSyncUniformCircuitInstance {
            witness: AtomicCell::new(Some(StorageApplicationCircuitInstanceWitness::<
                GoldilocksField,
            >::default())),
            config: Arc::new(geometry.cycles_per_storage_application as usize),
            round_function: Arc::new(Poseidon2Goldilocks),
            expected_public_input: None,
        }),
        ZkSyncBaseLayerCircuit::EventsSorter(ZkSyncUniformCircuitInstance {
            witness: AtomicCell::new(Some(
                EventsDeduplicatorInstanceWitness::<GoldilocksField>::default(),
            )),
            config: Arc::new(geometry.cycles_per_events_or_l1_messages_sorter as usize),
            round_function: Arc::new(Poseidon2Goldilocks),
            expected_public_input: None,
        }),
        ZkSyncBaseLayerCircuit::L1MessagesSorter(ZkSyncUniformCircuitInstance {
            witness: AtomicCell::new(Some(
                EventsDeduplicatorInstanceWitness::<GoldilocksField>::default(),
            )),
            config: Arc::new(geometry.cycles_per_events_or_l1_messages_sorter as usize),
            round_function: Arc::new(Poseidon2Goldilocks),
            expected_public_input: None,
        }),
        ZkSyncBaseLayerCircuit::L1MessagesHasher(ZkSyncUniformCircuitInstance {
            witness: AtomicCell::new(Some(
                LinearHasherCircuitInstanceWitness::<GoldilocksField>::default(),
            )),
            config: Arc::new(geometry.limit_for_l1_messages_pudata_hasher as usize),
            round_function: Arc::new(Poseidon2Goldilocks),
            expected_public_input: None,
        }),
    ]
}

pub fn generate_basic_circuit_vks() -> anyhow::Result<()> {
    let worker = Worker::new();
    let geometry = get_geometry_config();

    for circuit in get_all_basic_circuits(&geometry) {
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
