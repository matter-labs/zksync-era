use circuit_definitions::aux_definitions::witness_oracle::VmWitnessOracle;
use circuit_definitions::boojum::field::goldilocks::GoldilocksField;
use circuit_definitions::boojum::worker::Worker;
use circuit_definitions::circuit_definitions::base_layer::ZkSyncBaseLayerCircuit;
use circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursiveLayerCircuit;
use circuit_definitions::{
    ZkSyncDefaultRoundFunction, BASE_LAYER_CAP_SIZE, BASE_LAYER_FRI_LDE_FACTOR,
};
use structopt::StructOpt;
use zkevm_test_harness::geometry_config::get_geometry_config;
use zkevm_test_harness::prover_utils::{
    create_base_layer_setup_data, create_recursive_layer_setup_data,
};
use zksync_types::proofs::AggregationRound;
use zksync_vk_setup_data_server_fri::utils::{
    get_basic_circuits, get_leaf_circuits, get_node_circuit, get_scheduler_circuit, CYCLE_LIMIT,
};
use zksync_vk_setup_data_server_fri::{
    get_round_for_recursive_circuit_type, save_setup_data, ProverServiceDataKey, ProverSetupData,
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
    match opt.is_base_layer {
        true => {
            let circuit = get_base_layer_circuit(opt.numeric_circuit);
            generate_base_layer_setup_data(circuit);
        }
        false => {
            let circuit = get_recursive_circuit(opt.numeric_circuit);
            generate_recursive_layer_setup_data(circuit);
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

fn generate_recursive_layer_setup_data(circuit: ZkSyncRecursiveLayerCircuit) {
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
    let prover_setup_data = ProverSetupData {
        setup_base,
        setup,
        vk: vk.clone(),
        setup_tree,
        vars_hint,
        wits_hint,
        finalization_hint,
    };
    let serialized = bincode::serialize(&prover_setup_data).expect("Failed serializing setup data");
    let round = get_round_for_recursive_circuit_type(circuit_type);
    save_setup_data(ProverServiceDataKey::new(circuit_type, round), &serialized);
}

fn generate_base_layer_setup_data(
    circuit: ZkSyncBaseLayerCircuit<
        GoldilocksField,
        VmWitnessOracle<GoldilocksField>,
        ZkSyncDefaultRoundFunction,
    >,
) {
    let circuit_type = circuit.numeric_circuit_type();
    vlog::info!(
        "starting setup data generator for base layer circuit: {}.",
        circuit_type
    );
    let worker = Worker::new();
    let (setup_base, setup, vk, setup_tree, vars_hint, wits_hint, finalization_hint) =
        create_base_layer_setup_data(
            circuit.clone(),
            &worker,
            BASE_LAYER_FRI_LDE_FACTOR,
            BASE_LAYER_CAP_SIZE,
        );
    let prover_setup_data = ProverSetupData {
        setup_base,
        setup,
        vk: vk.clone(),
        setup_tree,
        vars_hint,
        wits_hint,
        finalization_hint,
    };
    let serialized = bincode::serialize(&prover_setup_data).expect("Failed serializing setup data");
    save_setup_data(
        ProverServiceDataKey::new(circuit_type, AggregationRound::BasicCircuits),
        &serialized,
    );
}
