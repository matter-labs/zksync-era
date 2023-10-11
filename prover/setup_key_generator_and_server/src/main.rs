#![cfg_attr(not(feature = "gpu"), allow(unused_imports))]

use anyhow::Context as _;
use std::env;
use std::fs::File;
use structopt::StructOpt;
use zkevm_test_harness::abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit;
use zkevm_test_harness::bellman::bn256::Bn256;
use zkevm_test_harness::witness::oracle::VmWitnessOracle;
use zksync_setup_key_server::{get_circuits_for_vk, get_setup_key_write_file_path};

#[cfg(feature = "gpu")]
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

#[cfg(not(feature = "gpu"))]
fn main() {
    unimplemented!("This binary is only available with `gpu` feature enabled");
}

#[cfg(feature = "gpu")]
fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();
    env::set_var("CRS_FILE", "setup_2^26.key");
    tracing::info!("Starting setup key generation!");
    get_circuits_for_vk()
        .context("get_circuits_for_vk()")?
        .into_iter()
        .filter(|c| c.numeric_circuit_type() == opt.numeric_circuit)
        .for_each(generate_setup_key_for_circuit);
    Ok(())
}

#[cfg(feature = "gpu")]
fn generate_setup_key_for_circuit(circuit: ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>) {
    use prover_service::utils::generate_setup_for_circuit;

    let mut prover = api::Prover::new();
    let setup = generate_setup_for_circuit(&mut prover, &circuit);
    save_setup_for_circuit_type(circuit.numeric_circuit_type(), setup);
    tracing::info!(
        "Finished setup key generation for circuit {:?} (id {:?})",
        circuit.short_description(),
        circuit.numeric_circuit_type()
    );
}

#[cfg(feature = "gpu")]
fn save_setup_for_circuit_type(circuit_type: u8, setup: prover_service::Setup) {
    let filepath = get_setup_key_write_file_path(circuit_type);
    tracing::info!("saving setup key to: {}", filepath);
    let setup_file = File::create(&filepath).unwrap();
    setup
        .write(setup_file)
        .expect("Failed saving setup key to file.");
    let setup_file = File::open(filepath).expect("Unable to open file");
    let size = setup_file.metadata().unwrap().len() as f64 / (1024.0 * 1024.0);
    println!("Saved file size: {:?}MB", size);
}
