use crate::{get_vk_for_circuit_type, get_vks_for_basic_circuits, get_vks_for_commitment};
use itertools::Itertools;
use serde_json::Value;
use std::collections::HashMap;
use zksync_types::zkevm_test_harness::abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit;
use zksync_types::zkevm_test_harness::bellman::bn256::Bn256;
use zksync_types::zkevm_test_harness::bellman::plonk::better_better_cs::setup::VerificationKey;

use zksync_types::zkevm_test_harness::witness::oracle::VmWitnessOracle;

#[test]
fn test_get_vk_for_circuit_type() {
    for circuit_type in 1..=18 {
        get_vk_for_circuit_type(circuit_type);
    }
}

#[test]
fn test_get_vks_for_basic_circuits() {
    let circuit_type_to_vk = get_vks_for_basic_circuits();
    let circuit_types: Vec<u8> = circuit_type_to_vk.into_keys().sorted().collect::<Vec<u8>>();
    let expected: Vec<u8> = (3..=18).collect();
    assert_eq!(
        expected, circuit_types,
        "circuit types must be in the range [3, 17]"
    );
}

#[test]
fn test_get_vks_for_commitment() {
    let vk_5 = get_vk_for_circuit_type(5);
    let vk_2 = get_vk_for_circuit_type(2);
    let vk_3 = get_vk_for_circuit_type(3);
    let map = HashMap::from([
        (5u8, vk_5.clone()),
        (2u8, vk_2.clone()),
        (3u8, vk_3.clone()),
    ]);
    let vks = get_vks_for_commitment(map);
    let expected = vec![vk_2, vk_3, vk_5];
    compare_vks(
        expected,
        vks,
        "expected verification key to be in order 2, 3, 5",
    );
}

fn get_vk_json(vk: &VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>) -> Value {
    serde_json::to_value(vk).unwrap()
}

fn get_vk_jsons(
    vks: Vec<VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>>,
) -> Vec<Value> {
    vks.into_iter().map(|vk| get_vk_json(&vk)).collect()
}

fn compare_vks(
    first: Vec<VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>>,
    second: Vec<VerificationKey<Bn256, ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>>>,
    error_message: &str,
) {
    let first_json = get_vk_jsons(first);
    let second_json = get_vk_jsons(second);
    assert_eq!(first_json, second_json, "{:?}", error_message);
}
