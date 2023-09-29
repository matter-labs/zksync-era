use zksync_prover_utils::vk_commitment_helper::{
    get_toml_formatted_value, read_contract_toml, write_contract_toml,
};
use zksync_verification_key_server::generate_commitments;

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
    contract_doc["contracts"]["RECURSION_CIRCUITS_SET_VKS_HASH"] =
        get_toml_formatted_value(basic_circuit_commitment_hex);
    contract_doc["contracts"]["RECURSION_LEAF_LEVEL_VK_HASH"] =
        get_toml_formatted_value(leaf_aggregation_commitment_hex);
    contract_doc["contracts"]["RECURSION_NODE_LEVEL_VK_HASH"] =
        get_toml_formatted_value(node_aggregation_commitment_hex);
    vlog::info!("Updated toml content: {:?}", contract_doc.to_string());
    write_contract_toml(contract_doc);
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_read_and_update_contract_toml() {
        read_and_update_contract_toml();
    }
}
