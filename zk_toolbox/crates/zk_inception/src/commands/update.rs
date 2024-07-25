use std::path::Path;

use anyhow::Context;
use common::{
    git::{pull, submodule_update},
    logger,
    spinner::Spinner,
};
use config::{
    ChainConfig, EcosystemConfig, CONTRACTS_FILE, EN_CONFIG_FILE, GENERAL_FILE, GENESIS_FILE,
    SECRETS_FILE,
};
use xshell::Shell;

use crate::messages::{
    msg_diff_contracts_config, msg_diff_genesis_config, msg_diff_secrets, msg_updating_chain,
    MSG_CHAIN_NOT_FOUND_ERR, MSG_DIFF_EN_CONFIG, MSG_DIFF_GENERAL_CONFIG,
    MSG_PULLING_ZKSYNC_CODE_SPINNER, MSG_UPDATING_SUBMODULES_SPINNER, MSG_UPDATING_ZKSYNC,
    MSG_ZKSYNC_UPDATED,
};

#[derive(Default)]
struct ConfigDiff {
    pub value_diff: serde_yaml::Mapping,
    pub added_fields: serde_yaml::Mapping,
}

impl ConfigDiff {
    fn print(&self, msg: &str) {
        if self.value_diff.is_empty() && self.added_fields.is_empty() {
            return;
        }

        let mut diff = logger::object_to_string(&self.value_diff);
        diff.push_str(&logger::object_to_string(&self.added_fields));

        logger::warn(msg);
        logger::warn(diff);
    }

    fn reset_value_diff(&mut self) {
        self.value_diff = serde_yaml::Mapping::new();
    }
}

pub fn run(shell: &Shell) -> anyhow::Result<()> {
    logger::info(MSG_UPDATING_ZKSYNC);
    let ecosystem = EcosystemConfig::from_file(shell)?;
    let link_to_code = ecosystem.link_to_code.clone();

    let spinner = Spinner::new(MSG_PULLING_ZKSYNC_CODE_SPINNER);
    pull(shell, link_to_code.clone())?;
    spinner.finish();
    let spinner = Spinner::new(MSG_UPDATING_SUBMODULES_SPINNER);
    submodule_update(shell, link_to_code.clone())?;
    spinner.finish();

    let general_config = serde_yaml::from_str(
        &shell.read_file(ecosystem.get_default_configs_path().join(GENERAL_FILE))?,
    )?;
    let external_node_config = serde_yaml::from_str(
        &shell.read_file(ecosystem.get_default_configs_path().join(EN_CONFIG_FILE))?,
    )?;
    let genesis_config = serde_yaml::from_str(
        &shell.read_file(ecosystem.get_default_configs_path().join(GENESIS_FILE))?,
    )?;
    let contracts_config = serde_yaml::from_str(
        &shell.read_file(ecosystem.get_default_configs_path().join(CONTRACTS_FILE))?,
    )?;
    let secrets_path = ecosystem.get_default_configs_path().join(SECRETS_FILE);
    let secrets = serde_yaml::from_str(&shell.read_file(secrets_path.clone())?)?;

    for chain in ecosystem.list_of_chains() {
        logger::step(msg_updating_chain(&chain));
        let chain = ecosystem
            .load_chain(Some(chain))
            .context(MSG_CHAIN_NOT_FOUND_ERR)?;
        update_chain(
            shell,
            &chain,
            &general_config,
            &external_node_config,
            &genesis_config,
            &contracts_config,
            &secrets,
            &secrets_path,
        )?;
    }

    logger::outro(MSG_ZKSYNC_UPDATED);

    Ok(())
}

fn merge_yaml_internal(
    a: &mut serde_yaml::Value,
    b: serde_yaml::Value,
    current_key: serde_yaml::Value,
    diff: &mut ConfigDiff,
) -> anyhow::Result<()> {
    match (a, b) {
        (serde_yaml::Value::Mapping(a), serde_yaml::Value::Mapping(b)) => {
            for (key, value) in b {
                if a.contains_key(&key) {
                    merge_yaml_internal(a.get_mut(&key).unwrap(), value, key, diff)?;
                } else {
                    a.insert(key.clone(), value.clone());
                    diff.added_fields.insert(key, value);
                }
            }
        }
        (a, b) => {
            if a != &b {
                diff.value_diff.insert(current_key, b);
            }
        }
    }
    Ok(())
}

fn merge_yaml(a: &mut serde_yaml::Value, b: serde_yaml::Value) -> anyhow::Result<ConfigDiff> {
    let mut diff = ConfigDiff::default();
    merge_yaml_internal(a, b, "".into(), &mut diff)?;
    Ok(diff)
}

fn save_updated_config(
    shell: &Shell,
    config: serde_yaml::Value,
    path: &Path,
    diff: ConfigDiff,
    msg: &str,
) -> anyhow::Result<()> {
    if diff.added_fields.is_empty() {
        return Ok(());
    }

    logger::info(msg);
    logger::info(logger::object_to_string(&diff.added_fields));

    let general_config = serde_yaml::to_string(&config)?;
    shell.write_file(path, general_config)?;

    Ok(())
}

fn update_chain(
    shell: &Shell,
    chain: &ChainConfig,
    general: &serde_yaml::Value,
    external_node: &serde_yaml::Value,
    genesis: &serde_yaml::Value,
    contracts: &serde_yaml::Value,
    secrets: &serde_yaml::Value,
    secrets_path: &Path,
) -> anyhow::Result<()> {
    let current_general_config_path = chain.path_to_general_config();
    let mut current_general_config =
        serde_yaml::from_str(&shell.read_file(current_general_config_path.clone())?)?;
    let diff = merge_yaml(&mut current_general_config, general.clone())?;
    save_updated_config(
        shell,
        current_general_config,
        &current_general_config_path,
        diff,
        MSG_DIFF_GENERAL_CONFIG,
    )?;

    let curret_external_node_config_path = chain.path_to_external_node_config();
    let mut current_external_node_config =
        serde_yaml::from_str(&shell.read_file(curret_external_node_config_path.clone())?)?;
    let diff = merge_yaml(&mut current_external_node_config, external_node.clone())?;
    save_updated_config(
        shell,
        current_external_node_config,
        &curret_external_node_config_path,
        diff,
        MSG_DIFF_EN_CONFIG,
    )?;

    let mut current_genesis_config =
        serde_yaml::from_str(&shell.read_file(chain.path_to_genesis_config())?)?;
    let diff = merge_yaml(&mut current_genesis_config, genesis.clone())?;
    diff.print(&msg_diff_genesis_config(&chain.name));

    let mut current_contracts_config =
        serde_yaml::from_str(&shell.read_file(chain.path_to_contracts_config())?)?;
    let diff = merge_yaml(&mut current_contracts_config, contracts.clone())?;
    diff.print(&msg_diff_contracts_config(&chain.name));

    let mut current_secrets =
        serde_yaml::from_str(&shell.read_file(chain.path_to_secrets_config())?)?;
    let mut diff = merge_yaml(&mut current_secrets, secrets.clone())?;
    diff.reset_value_diff(); // Values are expected to be different
    diff.print(&msg_diff_secrets(
        &chain.name,
        &chain.path_to_secrets_config(),
        secrets_path,
    ));

    Ok(())
}
