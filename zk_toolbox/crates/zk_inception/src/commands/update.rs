use std::path::Path;

use anyhow::Context;
use common::{
    git::{pull, submodule_update},
    logger,
    spinner::Spinner,
};
use config::{ChainConfig, EcosystemConfig, GENERAL_FILE, GENESIS_FILE};
use xshell::Shell;

use crate::messages::{
    msg_diff_genesis_config, msg_updating_chain, MSG_CHAIN_NOT_FOUND_ERR,
    MSG_PULLING_ZKSYNC_CODE_SPINNER, MSG_SHOW_DIFF, MSG_SHOW_DIFF_ADDED_FIELDS,
    MSG_UPDATING_GENERAL_CONFIG, MSG_UPDATING_SUBMODULES_SPINNER, MSG_UPDATING_ZKSYNC,
    MSG_ZKSYNC_UPDATED,
};

#[derive(Default)]
struct ConfigDiff {
    pub value_diff: serde_yaml::Mapping,
    pub added_fields: serde_yaml::Mapping,
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

    let genesis_config = serde_yaml::from_str(
        &shell.read_file(ecosystem.get_default_configs_path().join(GENESIS_FILE))?,
    )?;

    for chain in ecosystem.list_of_chains() {
        logger::info(msg_updating_chain(&chain));
        let chain = ecosystem
            .load_chain(Some(chain))
            .context(MSG_CHAIN_NOT_FOUND_ERR)?;
        update_chain(shell, &chain, &general_config, &genesis_config)?;
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
) -> anyhow::Result<()> {
    if diff.added_fields.is_empty() {
        return Ok(());
    }

    logger::info(MSG_SHOW_DIFF_ADDED_FIELDS);
    for (key, value) in diff.added_fields {
        let key = key.as_str().unwrap();
        logger::info(format!("{}: {:?}", key, value));
    }

    let general_config = serde_yaml::to_string(&config)?;
    shell.write_file(path, general_config)?;

    Ok(())
}

fn check_diff(diff: &ConfigDiff, msg: &str) -> anyhow::Result<()> {
    if diff.value_diff.is_empty() && diff.added_fields.is_empty() {
        return Ok(());
    }

    logger::warn(msg);
    logger::info(MSG_SHOW_DIFF);
    for (key, value) in diff.added_fields.iter() {
        let key = key.as_str().unwrap();
        logger::info(format!("{}: {:?}", key, value));
    }
    for (key, value) in diff.value_diff.iter() {
        let key = key.as_str().unwrap();
        logger::info(format!("{}: {:?}", key, value));
    }

    Ok(())
}

fn update_chain(
    shell: &Shell,
    chain: &ChainConfig,
    general: &serde_yaml::Value,
    genesis: &serde_yaml::Value,
) -> anyhow::Result<()> {
    logger::info(MSG_UPDATING_GENERAL_CONFIG);
    let current_general_config_path = chain.path_to_general_config();
    let mut current_general_config =
        serde_yaml::from_str(&shell.read_file(current_general_config_path.clone())?)?;
    let diff = merge_yaml(&mut current_general_config, general.clone())?;
    save_updated_config(
        shell,
        current_general_config,
        &current_general_config_path,
        diff,
    )?;

    let mut current_genesis_config =
        serde_yaml::from_str(&shell.read_file(chain.path_to_genesis_config())?)?;
    let diff = merge_yaml(&mut current_genesis_config, genesis.clone())?;
    check_diff(&diff, &msg_diff_genesis_config(&chain.name))?;

    Ok(())
}
