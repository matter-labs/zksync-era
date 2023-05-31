use vm::zk_evm::ethereum_types::U256;
use zksync_object_store::{CircuitKey, ObjectStore};
use zksync_types::zkevm_test_harness::abstract_zksync_circuit::concrete_circuits::ZkSyncCircuit;
use zksync_types::zkevm_test_harness::bellman::bn256::Bn256;
use zksync_types::zkevm_test_harness::witness::oracle::VmWitnessOracle;
use zksync_types::USED_BOOTLOADER_MEMORY_BYTES;
use zksync_types::{proofs::AggregationRound, L1BatchNumber};

pub fn expand_bootloader_contents(packed: Vec<(usize, U256)>) -> Vec<u8> {
    let mut result: Vec<u8> = Vec::new();
    result.resize(USED_BOOTLOADER_MEMORY_BYTES, 0);

    for (offset, value) in packed {
        value.to_big_endian(&mut result[(offset * 32)..(offset + 1) * 32]);
    }

    result.to_vec()
}

pub fn save_prover_input_artifacts(
    block_number: L1BatchNumber,
    circuits: &[ZkSyncCircuit<Bn256, VmWitnessOracle<Bn256>>],
    object_store: &dyn ObjectStore,
    aggregation_round: AggregationRound,
) -> Vec<(&'static str, String)> {
    let types_and_urls = circuits
        .iter()
        .enumerate()
        .map(|(sequence_number, circuit)| {
            let circuit_type = circuit.short_description();
            let circuit_key = CircuitKey {
                block_number,
                sequence_number,
                circuit_type,
                aggregation_round,
            };
            let blob_url = object_store.put(circuit_key, circuit).unwrap();
            (circuit_type, blob_url)
        });
    types_and_urls.collect()
}
