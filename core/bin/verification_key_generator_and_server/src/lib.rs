use std::collections::HashMap;
use std::path::Path;
use zksync_types::zkevm_test_harness::abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit;
use zksync_types::zkevm_test_harness::bellman::bn256::Bn256;
use zksync_types::zkevm_test_harness::bellman::plonk::better_better_cs::setup::VerificationKey;
use zksync_types::zkevm_test_harness::witness::oracle::VmWitnessOracle;

use itertools::Itertools;
use zksync_types::circuit::{
    GEOMETRY_CONFIG, LEAF_SPLITTING_FACTOR, NODE_SPLITTING_FACTOR, SCHEDULER_UPPER_BOUND,
};
use zksync_types::zkevm_test_harness::witness::full_block_artifact::BlockBasicCircuits;
use zksync_types::zkevm_test_harness::witness::recursive_aggregation::padding_aggregations;
use zksync_types::zkevm_test_harness::witness::vk_set_generator::circuits_for_vk_generation;

#[cfg(test)]
mod tests;

pub fn get_vks_for_basic_circuits(
) -> HashMap<u8, VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>> {
    // 3-17 are the ids of basic circuits
    (3..=18)
        .map(|circuit_type| (circuit_type, get_vk_for_circuit_type(circuit_type)))
        .collect()
}

pub fn get_vk_for_circuit_type(
    circuit_type: u8,
) -> VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>> {
    let filepath = get_file_path(circuit_type);
    vlog::info!("Fetching verification key from path: {}", filepath);
    let text = std::fs::read_to_string(&filepath)
        .unwrap_or_else(|_| panic!("Failed reading verification key from path: {}", filepath));
    serde_json::from_str::<VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>>(
        &text,
    )
    .unwrap_or_else(|_| {
        panic!(
            "Failed deserializing verification key from path: {}",
            filepath
        )
    })
}

pub fn save_vk_for_circuit_type(
    circuit_type: u8,
    vk: VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
) {
    let filepath = get_file_path(circuit_type);
    vlog::info!("saving verification key to: {}", filepath);
    std::fs::write(filepath, serde_json::to_string_pretty(&vk).unwrap()).unwrap();
}

pub fn get_ordered_vks_for_basic_circuits(
    circuits: &BlockBasicCircuits<Bn256>,
    verification_keys: &HashMap<
        u8,
        VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
    >,
) -> Vec<VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>> {
    circuits
        .clone()
        .into_flattened_set()
        .iter()
        .map(|circuit| {
            let circuit_id = circuit.numeric_circuit_type();
            verification_keys
                .get(&circuit_id)
                .unwrap_or_else(|| {
                    panic!("no VK for circuit number {:?}", circuit.short_description())
                })
                .clone()
        })
        .collect()
}

pub fn get_vks_for_commitment(
    verification_keys: HashMap<
        u8,
        VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>,
    >,
) -> Vec<VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>> {
    // We need all the vks sorted by their respective circuit ids
    verification_keys
        .into_iter()
        .sorted_by_key(|(id, _)| *id)
        .map(|(_, val)| val)
        .collect()
}

pub fn get_circuits_for_vk() -> Vec<ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>> {
    ensure_setup_key_exist();
    let padding_aggregations = padding_aggregations(NODE_SPLITTING_FACTOR);
    circuits_for_vk_generation(
        GEOMETRY_CONFIG,
        LEAF_SPLITTING_FACTOR,
        NODE_SPLITTING_FACTOR,
        SCHEDULER_UPPER_BOUND,
        padding_aggregations,
    )
}

fn ensure_setup_key_exist() {
    if !Path::new("setup_2^26.key").exists() {
        panic!("File setup_2^26.key is required to be present in current directory for verification keys generation. \ndownload from https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_2^26.key");
    }
}
fn get_file_path(circuit_type: u8) -> String {
    let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| "/".into());
    format!(
        "{}/core/bin/verification_key_generator_and_server/data/verification_{}_key.json",
        zksync_home, circuit_type
    )
}
