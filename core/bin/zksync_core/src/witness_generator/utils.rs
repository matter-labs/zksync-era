use vm::zk_evm::abstractions::MEMORY_CELLS_OTHER_PAGES;
use vm::zk_evm::ethereum_types::U256;
use zksync_object_store::gcs_utils::prover_circuit_input_blob_url;
use zksync_object_store::object_store::{DynamicObjectStore, PROVER_JOBS_BUCKET_PATH};
use zksync_types::{proofs::AggregationRound, L1BatchNumber};

pub fn expand_bootloader_contents(packed: Vec<(usize, U256)>) -> Vec<u8> {
    let mut result: [u8; MEMORY_CELLS_OTHER_PAGES] = [0; MEMORY_CELLS_OTHER_PAGES];

    for (offset, value) in packed {
        value.to_big_endian(&mut result[(offset * 32)..(offset + 1) * 32]);
    }

    result.to_vec()
}

pub async fn save_prover_input_artifacts(
    block_number: L1BatchNumber,
    serialized_circuits: Vec<(String, Vec<u8>)>,
    object_store: &mut DynamicObjectStore,
    aggregation_round: AggregationRound,
) {
    for (sequence_number, (circuit, input)) in serialized_circuits.into_iter().enumerate() {
        let circuit_input_blob_url = prover_circuit_input_blob_url(
            block_number,
            sequence_number,
            circuit,
            aggregation_round,
        );
        object_store
            .put(PROVER_JOBS_BUCKET_PATH, circuit_input_blob_url, input)
            .unwrap();
    }
}
