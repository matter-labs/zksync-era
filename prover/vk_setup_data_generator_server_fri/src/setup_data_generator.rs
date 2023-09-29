use zksync_prover_fri_types::circuit_definitions::aux_definitions::witness_oracle::VmWitnessOracle;
use zksync_prover_fri_types::circuit_definitions::boojum::field::goldilocks::GoldilocksField;
use zksync_prover_fri_types::circuit_definitions::boojum::worker::Worker;
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::base_layer::ZkSyncBaseLayerCircuit;
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursiveLayerCircuit;
use zksync_prover_fri_types::circuit_definitions::{
    ZkSyncDefaultRoundFunction, BASE_LAYER_CAP_SIZE, BASE_LAYER_FRI_LDE_FACTOR,
};

use structopt::StructOpt;
use zkevm_test_harness::geometry_config::get_geometry_config;
use zkevm_test_harness::prover_utils::create_recursive_layer_setup_data;
use zksync_types::proofs::AggregationRound;
use zksync_vk_setup_data_server_fri::generate_cpu_base_layer_setup_data;
use zksync_vk_setup_data_server_fri::utils::{
    get_basic_circuits, get_leaf_circuits, get_node_circuit, get_scheduler_circuit, CYCLE_LIMIT,
};
use zksync_vk_setup_data_server_fri::{
    get_finalization_hints, get_recursive_layer_vk_for_circuit_type,
    get_round_for_recursive_circuit_type, save_setup_data, GoldilocksProverSetupData,
    ProverSetupData,
};

use zksync_prover_fri_types::ProverServiceDataKey;
#[cfg(feature = "gpu")]
use {
    shivini::cs::setup::GpuSetup, shivini::ProverContext,
    zksync_vk_setup_data_server_fri::GpuProverSetupData,
};

#[derive(Debug, StructOpt)]
#[structopt(
    name = "Generate setup data for individual circuit",
    about = "Tool for generating setup data for individual circuit"
)]
struct Opt {
    /// Numeric circuit type valid value are
    /// 1. for base layer [1-13].
    /// 2. for recursive layer [1-15].
    #[structopt(long)]
    numeric_circuit: u8,
    /// Boolean representing whether to generate for base layer or for recursive layer.
    #[structopt(short = "b", long = "is_base_layer")]
    is_base_layer: bool,
}

fn main() {
    let opt = Opt::from_args();

    #[cfg(feature = "gpu")]
    generate_gpu_setup_data(opt.is_base_layer, opt.numeric_circuit);

    #[cfg(not(feature = "gpu"))]
    generate_cpu_setup_data(opt.is_base_layer, opt.numeric_circuit);
}

fn generate_cpu_setup_data(is_base_layer: bool, numeric_circuit: u8) {
    match is_base_layer {
        true => {
            let circuit = get_base_layer_circuit(numeric_circuit);
            let prover_setup_data = generate_cpu_base_layer_setup_data(circuit);
            let serialized =
                bincode::serialize(&prover_setup_data).expect("Failed serializing setup data");
            save_setup_data(
                ProverServiceDataKey::new(numeric_circuit, AggregationRound::BasicCircuits),
                &serialized,
            );
        }
        false => {
            let circuit = get_recursive_circuit(numeric_circuit);
            let prover_setup_data = generate_cpu_recursive_layer_setup_data(circuit);
            let serialized =
                bincode::serialize(&prover_setup_data).expect("Failed serializing setup data");
            let round = get_round_for_recursive_circuit_type(numeric_circuit);
            save_setup_data(
                ProverServiceDataKey::new(numeric_circuit, round),
                &serialized,
            );
        }
    }
}

fn get_base_layer_circuit(
    id: u8,
) -> ZkSyncBaseLayerCircuit<
    GoldilocksField,
    VmWitnessOracle<GoldilocksField>,
    ZkSyncDefaultRoundFunction,
> {
    get_basic_circuits(CYCLE_LIMIT, get_geometry_config())
        .into_iter()
        .find(|circuit| id == circuit.numeric_circuit_type())
        .unwrap_or_else(|| panic!("No basic circuit found for id: {}", id))
}

fn get_recursive_circuit(id: u8) -> ZkSyncRecursiveLayerCircuit {
    let mut recursive_circuits = get_leaf_circuits();
    recursive_circuits.push(get_node_circuit());
    recursive_circuits.push(get_scheduler_circuit());
    recursive_circuits
        .into_iter()
        .find(|circuit| id == circuit.numeric_circuit_type())
        .unwrap_or_else(|| panic!("No recursive circuit found for id: {}", id))
}

fn generate_cpu_recursive_layer_setup_data(
    circuit: ZkSyncRecursiveLayerCircuit,
) -> GoldilocksProverSetupData {
    let circuit_type = circuit.numeric_circuit_type();
    vlog::info!(
        "starting setup data generator for recursive layer circuit: {}.",
        circuit_type
    );
    let worker = Worker::new();
    let (setup_base, setup, vk, setup_tree, vars_hint, wits_hint, finalization_hint) =
        create_recursive_layer_setup_data(
            circuit.clone(),
            &worker,
            BASE_LAYER_FRI_LDE_FACTOR,
            BASE_LAYER_CAP_SIZE,
        );
    let key = ProverServiceDataKey::new(
        circuit_type,
        get_round_for_recursive_circuit_type(circuit_type),
    );
    let existing_finalization_hint = get_finalization_hints(key);
    assert_eq!(
        existing_finalization_hint, finalization_hint,
        "finalization hint mismatch for circuit: {circuit_type}"
    );
    let existing_vk = get_recursive_layer_vk_for_circuit_type(circuit_type);
    assert_eq!(
        existing_vk.into_inner(),
        vk,
        "vk mismatch for circuit: {circuit_type}"
    );
    ProverSetupData {
        setup_base,
        setup,
        vk: vk.clone(),
        setup_tree,
        vars_hint,
        wits_hint,
        finalization_hint,
    }
}

#[cfg(feature = "gpu")]
fn generate_gpu_setup_data(is_base_layer: bool, numeric_circuit: u8) {
    let context = ProverContext::create().expect("failed initializing gpu prover context");
    let (cpu_setup_data, round) = match is_base_layer {
        true => {
            let circuit = get_base_layer_circuit(numeric_circuit);
            (
                generate_cpu_base_layer_setup_data(circuit.clone()),
                AggregationRound::BasicCircuits,
            )
        }
        false => {
            let circuit = get_recursive_circuit(numeric_circuit);
            (
                generate_cpu_recursive_layer_setup_data(circuit.clone()),
                get_round_for_recursive_circuit_type(numeric_circuit),
            )
        }
    };
    let gpu_setup_data = GpuSetup::from_setup_and_hints(
        cpu_setup_data.setup_base,
        cpu_setup_data.setup_tree,
        cpu_setup_data.vars_hint.clone(),
    )
    .expect("failed creating GPU base layer setup data");
    let gpu_prover_setup_data = GpuProverSetupData {
        setup: gpu_setup_data,
        vk: cpu_setup_data.vk,
        finalization_hint: cpu_setup_data.finalization_hint,
    };
    let serialized =
        bincode::serialize(&gpu_prover_setup_data).expect("Failed serializing setup data");
    save_setup_data(
        ProverServiceDataKey::new(numeric_circuit, round),
        &serialized,
    );
}
