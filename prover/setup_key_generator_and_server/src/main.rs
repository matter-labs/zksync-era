use api::Prover;
use prover_service::utils::generate_setup_for_circuit;
use prover_service::Setup;
use std::env;
use std::fs::File;
use structopt::StructOpt;
use zkevm_test_harness::abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit;
use zkevm_test_harness::bellman::bn256::Bn256;
use zkevm_test_harness::witness::oracle::VmWitnessOracle;
use zksync_setup_key_server::{get_circuits_for_vk, get_setup_key_write_file_path};

#[derive(Debug, StructOpt)]
#[structopt(
    name = "Generate setup keys for individual circuit",
    about = "Tool for generating setup key for individual circuit"
)]
struct Opt {
    /// Numeric circuit type valid value from [0-17].
    #[structopt(long)]
    numeric_circuit: u8,
}

fn main() {
    let opt = Opt::from_args();
    env::set_var("CRS_FILE", "setup_2^26.key");
    vlog::info!("Starting setup key generation!");
    get_circuits_for_vk()
        .into_iter()
        .filter(|c| c.numeric_circuit_type() == opt.numeric_circuit)
        .for_each(generate_setup_key_for_circuit);
}

fn generate_setup_key_for_circuit(circuit: ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>) {
    let mut prover = Prover::new();
    let setup = generate_setup_for_circuit(&mut prover, &circuit);
    save_setup_for_circuit_type(circuit.numeric_circuit_type(), setup);
    vlog::info!(
        "Finished setup key generation for circuit {:?} (id {:?})",
        circuit.short_description(),
        circuit.numeric_circuit_type()
    );
}

fn save_setup_for_circuit_type(circuit_type: u8, setup: Setup) {
    let filepath = get_setup_key_write_file_path(circuit_type);
    vlog::info!("saving setup key to: {}", filepath);
    let setup_file = File::create(&filepath).unwrap();
    setup
        .write(setup_file)
        .expect("Failed saving setup key to file.");
    let setup_file = File::open(filepath).expect("Unable to open file");
    let size = setup_file.metadata().unwrap().len() as f64 / (1024.0 * 1024.0);
    println!("Saved file size: {:?}MB", size);
}
