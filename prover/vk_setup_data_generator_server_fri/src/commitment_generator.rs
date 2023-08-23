use zkevm_test_harness::witness::recursive_aggregation::{
    compute_leaf_vks_and_params_commitment, compute_node_vk_commitment,
};
use zksync_prover_fri_types::circuit_definitions::circuit_definitions::recursion_layer::ZkSyncRecursionLayerStorageType;
use zksync_prover_utils::vk_commitment_helper::{
    get_toml_formatted_value, read_contract_toml, write_contract_toml,
};
use zksync_vk_setup_data_server_fri::get_recursive_layer_vk_for_circuit_type;
use zksync_vk_setup_data_server_fri::utils::get_leaf_vk_params;

fn main() {
    vlog::info!("Starting commitment generation!");
    read_and_update_contract_toml();
}

fn read_and_update_contract_toml() {
    let mut contract_doc = read_contract_toml();
    let (leaf_aggregation_commitment_hex, node_aggregation_commitment_hex) = generate_commitments();
    contract_doc["contracts"]["FRI_VK_COMMITMENT_LEAF"] =
        get_toml_formatted_value(leaf_aggregation_commitment_hex);
    contract_doc["contracts"]["FRI_VK_COMMITMENT_NODE"] =
        get_toml_formatted_value(node_aggregation_commitment_hex);
    vlog::info!("Updated toml content: {:?}", contract_doc.to_string());
    write_contract_toml(contract_doc);
}

fn generate_commitments() -> (String, String) {
    let leaf_vk_params = get_leaf_vk_params();
    let leaf_layer_params = leaf_vk_params
        .iter()
        .map(|el| el.1.clone())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    let leaf_vk_commitment = compute_leaf_vks_and_params_commitment(leaf_layer_params);

    let node_vk = get_recursive_layer_vk_for_circuit_type(
        ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
    );
    let node_vk_commitment = compute_node_vk_commitment(node_vk.clone());

    let leaf_aggregation_commitment_hex = format!("{:?}", leaf_vk_commitment);
    let node_aggregation_commitment_hex = format!("{:?}", node_vk_commitment);
    vlog::info!("leaf aggregation commitment {:?}", node_vk_commitment);
    vlog::info!("node aggregation commitment {:?}", node_vk_commitment);
    (
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
