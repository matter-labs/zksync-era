use anyhow::Context;
use zksync_vk_setup_data_server_fri::{
    commitment_utils::generate_commitments,
    vk_commitment_helper::{get_toml_formatted_value, read_contract_toml, write_contract_toml},
};

fn main() -> anyhow::Result<()> {
    tracing::info!("Starting commitment generation!");
    read_and_update_contract_toml()
}

fn read_and_update_contract_toml() -> anyhow::Result<()> {
    let mut contract_doc = read_contract_toml().context("read_contract_toml()")?;
    let vk_commitments = generate_commitments().context("generate_commitments()")?;
    contract_doc["contracts"]["FRI_RECURSION_LEAF_LEVEL_VK_HASH"] =
        get_toml_formatted_value(vk_commitments.leaf);
    contract_doc["contracts"]["FRI_RECURSION_NODE_LEVEL_VK_HASH"] =
        get_toml_formatted_value(vk_commitments.node);
    contract_doc["contracts"]["FRI_RECURSION_SCHEDULER_LEVEL_VK_HASH"] =
        get_toml_formatted_value(vk_commitments.scheduler);
    tracing::info!("Updated toml content: {:?}", contract_doc.to_string());
    write_contract_toml(contract_doc).context("write_contract_toml")
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_read_and_update_contract_toml() {
        read_and_update_contract_toml().unwrap();
    }
}
