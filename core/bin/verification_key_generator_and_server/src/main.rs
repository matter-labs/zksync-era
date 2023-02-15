use serde_json::Value;
use std::collections::HashSet;
use std::env;
use std::fs::File;
use std::io::{BufReader, Read};
use std::iter::FromIterator;
use zksync_types::zkevm_test_harness::abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit;
use zksync_types::zkevm_test_harness::bellman::bn256::Bn256;
use zksync_types::zkevm_test_harness::bellman::plonk::better_better_cs::cs::PlonkCsWidth4WithNextStepAndCustomGatesParams;
use zksync_types::zkevm_test_harness::witness::oracle::VmWitnessOracle;
use zksync_verification_key_server::{get_circuits_for_vk, save_vk_for_circuit_type};

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut circuit_types: HashSet<u8> = (3..17).collect();
    if args.len() > 1 {
        circuit_types = HashSet::from_iter([get_and_ensure_valid_circuit_type(args[1].clone())]);
    }
    vlog::info!("Starting verification key generation!");
    get_circuits_for_vk()
        .into_iter()
        .filter(|c| circuit_types.contains(&c.numeric_circuit_type()))
        .for_each(generate_verification_key);
}

fn get_and_ensure_valid_circuit_type(circuit_type: String) -> u8 {
    vlog::info!("Received circuit_type: {:?}", circuit_type);
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
    vlog::info!(
        "Finished VK generation for circuit {:?} (id {:?})",
        circuit.short_description(),
        circuit.numeric_circuit_type()
    );
}

fn _extract_keys_from_json_file(filepath: &str) {
    let mut file = File::open(filepath).unwrap();
    let mut data = String::new();
    file.read_to_string(&mut data).unwrap();
    let json: Vec<serde_json::Value> = serde_json::from_str(&data).expect("malformed JSON");
    for mut item in json {
        let kv = item.as_object_mut().unwrap();
        _build_and_save_verification_key(kv);
    }
}

fn _build_and_save_verification_key(kv: &mut serde_json::Map<String, Value>) {
    let key: &str = kv
        .get("key")
        .expect("key must be present in json")
        .as_str()
        .expect("key must be of type string");
    let circuit_type: u8 = key
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse::<u8>()
        .unwrap();
    let value: String = kv
        .get("value")
        .expect("value must be present in json")
        .as_str()
        .expect("value must be of type string")
        .replace("E'\\\\x", "")
        .replace('\'', "");
    let bytes = hex::decode(value).expect("Invalid hex string for verification key");
    let vk = bincode::deserialize_from(BufReader::new(bytes.as_slice())).unwrap();
    vlog::info!("Extracted circuit_type: {:?} vk : {:?}", circuit_type, vk);
    save_vk_for_circuit_type(circuit_type, vk);
}
