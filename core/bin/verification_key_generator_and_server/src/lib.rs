use ff::to_hex;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use zksync_types::zkevm_test_harness::abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit;
use zksync_types::zkevm_test_harness::bellman::bn256::Bn256;
use zksync_types::zkevm_test_harness::bellman::plonk::better_better_cs::setup::VerificationKey;
use zksync_types::zkevm_test_harness::witness::oracle::VmWitnessOracle;

use itertools::Itertools;
use structopt::lazy_static::lazy_static;
use zksync_types::circuit::SCHEDULER_CIRCUIT_INDEX;
use zksync_types::circuit::{
    GEOMETRY_CONFIG, LEAF_CIRCUIT_INDEX, LEAF_SPLITTING_FACTOR, NODE_CIRCUIT_INDEX,
    NODE_SPLITTING_FACTOR, SCHEDULER_UPPER_BOUND,
};
use zksync_types::protocol_version::{L1VerifierConfig, VerifierParams};
use zksync_types::vk_transform::generate_vk_commitment;
use zksync_types::zkevm_test_harness::witness;
use zksync_types::zkevm_test_harness::witness::full_block_artifact::BlockBasicCircuits;
use zksync_types::zkevm_test_harness::witness::recursive_aggregation::{
    erase_vk_type, padding_aggregations,
};
use zksync_types::zkevm_test_harness::witness::vk_set_generator::circuits_for_vk_generation;
use zksync_types::H256;

#[cfg(test)]
mod tests;

lazy_static! {
    static ref COMMITMENTS: Lazy<L1VerifierConfig> = Lazy::new(|| { circuit_commitments() });
}

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
    tracing::info!("Fetching verification key from path: {}", filepath);
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
    tracing::info!("saving verification key to: {}", filepath);
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

pub fn generate_commitments() -> (String, String, String) {
    let (_, basic_circuit_commitment, _) =
        witness::recursive_aggregation::form_base_circuits_committment(get_vks_for_commitment(
            get_vks_for_basic_circuits(),
        ));

    let leaf_aggregation_vk = get_vk_for_circuit_type(LEAF_CIRCUIT_INDEX);
    let node_aggregation_vk = get_vk_for_circuit_type(NODE_CIRCUIT_INDEX);

    let (_, leaf_aggregation_vk_commitment) =
        witness::recursive_aggregation::compute_vk_encoding_and_committment(erase_vk_type(
            leaf_aggregation_vk,
        ));

    let (_, node_aggregation_vk_commitment) =
        witness::recursive_aggregation::compute_vk_encoding_and_committment(erase_vk_type(
            node_aggregation_vk,
        ));
    let basic_circuit_commitment_hex = format!("0x{}", to_hex(&basic_circuit_commitment));
    let leaf_aggregation_commitment_hex = format!("0x{}", to_hex(&leaf_aggregation_vk_commitment));
    let node_aggregation_commitment_hex = format!("0x{}", to_hex(&node_aggregation_vk_commitment));
    tracing::info!(
        "basic circuit commitment {:?}",
        basic_circuit_commitment_hex
    );
    tracing::info!(
        "leaf aggregation commitment {:?}",
        leaf_aggregation_commitment_hex
    );
    tracing::info!(
        "node aggregation commitment {:?}",
        node_aggregation_commitment_hex
    );
    (
        basic_circuit_commitment_hex,
        leaf_aggregation_commitment_hex,
        node_aggregation_commitment_hex,
    )
}

fn circuit_commitments() -> L1VerifierConfig {
    let (basic, leaf, node) = generate_commitments();
    let scheduler = generate_vk_commitment(get_vk_for_circuit_type(SCHEDULER_CIRCUIT_INDEX));
    L1VerifierConfig {
        params: VerifierParams {
            recursion_node_level_vk_hash: H256::from_str(&node).expect("invalid node commitment"),
            recursion_leaf_level_vk_hash: H256::from_str(&leaf).expect("invalid leaf commitment"),
            recursion_circuits_set_vks_hash: H256::from_str(&basic)
                .expect("invalid basic commitment"),
        },
        recursion_scheduler_level_vk_hash: scheduler,
    }
}

pub fn get_cached_commitments() -> L1VerifierConfig {
    **COMMITMENTS
}
