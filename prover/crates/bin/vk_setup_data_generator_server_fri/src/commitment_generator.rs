use anyhow::Context;
use zksync_prover_keystore::{commitment_utils::generate_commitments, keystore::Keystore};

use crate::vk_commitment_helper::{
    get_toml_formatted_value, read_contract_toml, write_contract_toml,
};

pub fn read_and_update_contract_toml(keystore: &Keystore, dryrun: bool) -> anyhow::Result<()> {
    let mut contract_doc = read_contract_toml().context("read_contract_toml()")?;
    let vk_commitments = generate_commitments(keystore).context("generate_commitments()")?;

    contract_doc["contracts"]["FRI_RECURSION_LEAF_LEVEL_VK_HASH"] =
        get_toml_formatted_value(vk_commitments.leaf);
    contract_doc["contracts"]["FRI_RECURSION_NODE_LEVEL_VK_HASH"] =
        get_toml_formatted_value(vk_commitments.node);
    contract_doc["contracts"]["FRI_RECURSION_SCHEDULER_LEVEL_VK_HASH"] =
        get_toml_formatted_value(vk_commitments.scheduler);
    contract_doc["contracts"]["SNARK_WRAPPER_VK_HASH"] =
        get_toml_formatted_value(vk_commitments.snark_wrapper);
    tracing::info!("Updated toml content: {:?}", contract_doc.to_string());
    if !dryrun {
        write_contract_toml(contract_doc).context("write_contract_toml")?;
    } else {
        tracing::warn!("DRY RUN - Not updating the file.")
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_read_and_update_contract_toml() {
        read_and_update_contract_toml(&Keystore::default(), true).unwrap();
    }
}
