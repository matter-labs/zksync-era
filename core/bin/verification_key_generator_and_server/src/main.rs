use std::collections::HashSet;
use std::env;
use zksync_types::zkevm_test_harness::abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit;
use zksync_types::zkevm_test_harness::bellman::bn256::Bn256;
use zksync_types::zkevm_test_harness::bellman::plonk::better_better_cs::cs::PlonkCsWidth4WithNextStepAndCustomGatesParams;
use zksync_types::zkevm_test_harness::witness::oracle::VmWitnessOracle;
use zksync_verification_key_server::{get_circuits_for_vk, save_vk_for_circuit_type};

/// Creates verification keys for the given circuit.
fn main() {
    let args: Vec<String> = env::args().collect();

    let circuit_types: HashSet<u8> = if args.len() > 1 {
        [get_and_ensure_valid_circuit_type(args[1].clone())].into()
    } else {
        (3..17).collect()
    };
    tracing::info!("Starting verification key generation!");
    get_circuits_for_vk()
        .into_iter()
        .filter(|c| circuit_types.contains(&c.numeric_circuit_type()))
        .for_each(generate_verification_key);
}

fn get_and_ensure_valid_circuit_type(circuit_type: String) -> u8 {
    tracing::info!("Received circuit_type: {:?}", circuit_type);
    circuit_type
        .parse::<u8>()
        .expect("Please specify a circuit type in range [1, 17]")
}

fn generate_verification_key(circuit: ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>) {
    let res = circuit_testing::create_vk_for_padding_size_log_2::<
        Bn256,
        _,
        PlonkCsWidth4WithNextStepAndCustomGatesParams,
    >(circuit.clone(), 26)
    .unwrap();
    save_vk_for_circuit_type(circuit.numeric_circuit_type(), res);
    tracing::info!(
        "Finished VK generation for circuit {:?} (id {:?})",
        circuit.short_description(),
        circuit.numeric_circuit_type()
    );
}
