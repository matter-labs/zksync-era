use zksync_types::proofs::AggregationRound;
use zksync_types::L1BatchNumber;

pub fn prover_circuit_input_blob_url(
    block_number: L1BatchNumber,
    sequence_number: usize,
    circuit_type: String,
    aggregation_round: AggregationRound,
) -> String {
    format!(
        "{}_{}_{}_{:?}.bin",
        block_number, sequence_number, circuit_type, aggregation_round
    )
}

pub fn merkle_tree_paths_blob_url(block_number: L1BatchNumber) -> String {
    format!("merkel_tree_paths_{}.bin", block_number)
}

pub fn basic_circuits_blob_url(block_number: L1BatchNumber) -> String {
    format!("basic_circuits_{}.bin", block_number)
}

pub fn basic_circuits_inputs_blob_url(block_number: L1BatchNumber) -> String {
    format!("basic_circuits_inputs_{}.bin", block_number)
}

pub fn leaf_layer_subqueues_blob_url(block_number: L1BatchNumber) -> String {
    format!("leaf_layer_subqueues_{}.bin", block_number)
}

pub fn aggregation_outputs_blob_url(block_number: L1BatchNumber) -> String {
    format!("aggregation_outputs_{}.bin", block_number)
}

pub fn scheduler_witness_blob_url(block_number: L1BatchNumber) -> String {
    format!("scheduler_witness_{}.bin", block_number)
}

pub fn final_node_aggregations_blob_url(block_number: L1BatchNumber) -> String {
    format!("final_node_aggregations_{}.bin", block_number)
}
