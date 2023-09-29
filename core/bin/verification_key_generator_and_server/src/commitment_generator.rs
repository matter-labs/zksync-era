use ff::to_hex;
use zksync_prover_utils::vk_commitment_helper::{
    get_toml_formatted_value, read_contract_toml, write_contract_toml,
};
use zksync_types::circuit::{LEAF_CIRCUIT_INDEX, NODE_CIRCUIT_INDEX};
use zksync_types::zkevm_test_harness::witness;
use zksync_types::zkevm_test_harness::witness::recursive_aggregation::erase_vk_type;
use zksync_verification_key_server::{
    get_vk_for_circuit_type, get_vks_for_basic_circuits, get_vks_for_commitment,
};

fn main() {
    vlog::info!("Starting commitment generation!");
    read_and_update_contract_toml();
}

fn read_and_update_contract_toml() {
    let mut contract_doc = read_contract_toml();
    let (
        basic_circuit_commitment_hex,
        leaf_aggregation_commitment_hex,
        node_aggregation_commitment_hex,
    ) = generate_commitments();
    contract_doc["contracts"]["VK_COMMITMENT_BASIC_CIRCUITS"] =
        get_toml_formatted_value(basic_circuit_commitment_hex);
    contract_doc["contracts"]["VK_COMMITMENT_LEAF"] =
        get_toml_formatted_value(leaf_aggregation_commitment_hex);
    contract_doc["contracts"]["VK_COMMITMENT_NODE"] =
        get_toml_formatted_value(node_aggregation_commitment_hex);
    vlog::info!("Updated toml content: {:?}", contract_doc.to_string());
    write_contract_toml(contract_doc);
}

fn generate_commitments() -> (String, String, String) {
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
    vlog::info!(
        "basic circuit commitment {:?}",
        basic_circuit_commitment_hex
    );
    vlog::info!(
        "leaf aggregation commitment {:?}",
        leaf_aggregation_commitment_hex
    );
    vlog::info!(
        "node aggregation commitment {:?}",
        node_aggregation_commitment_hex
    );
    (
        basic_circuit_commitment_hex,
        leaf_aggregation_commitment_hex,
        node_aggregation_commitment_hex,
    )
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_read_and_update_contract_toml() {
        read_and_update_contract_toml();
    }
}
