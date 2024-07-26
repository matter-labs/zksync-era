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

    let general_config_path = ecosystem.get_default_configs_path().join(GENERAL_FILE);
    let external_node_config_path = ecosystem.get_default_configs_path().join(EN_CONFIG_FILE);
    let genesis_config_path = ecosystem.get_default_configs_path().join(GENESIS_FILE);
    let contracts_config_path = ecosystem.get_default_configs_path().join(CONTRACTS_FILE);
    let secrets_path = ecosystem.get_default_configs_path().join(SECRETS_FILE);

    for chain in ecosystem.list_of_chains() {
        logger::step(msg_updating_chain(&chain));
        let chain = ecosystem
            .load_chain(Some(chain))
            .context(MSG_CHAIN_NOT_FOUND_ERR)?;
        update_chain(
            shell,
            &chain,
            &general_config_path,
            &external_node_config_path,
            &genesis_config_path,
            &contracts_config_path,
            &secrets_path,
        )?;
    }

    logger::outro(MSG_ZKSYNC_UPDATED);

    Ok(())
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

fn update_config(
    shell: Shell,
    original_config_path: &Path,
    chain_config_path: &Path,
    replace_config: bool,
    ignore_updated_values: bool,
    msg: &str,
) -> anyhow::Result<()> {
    let original_config = serde_yaml::from_str(&shell.read_file(original_config_path)?)?;
    let mut chain_config = serde_yaml::from_str(&shell.read_file(chain_config_path)?)?;
    let mut diff = merge_yaml(&mut chain_config, original_config)?;
    if replace_config {
        save_updated_config(&shell, chain_config, chain_config_path, diff, msg)?;
    } else {
        if ignore_updated_values {
            diff.reset_value_diff();
        }
        diff.print(msg);
    }

    Ok(())
}

fn update_chain(
    shell: &Shell,
    chain: &ChainConfig,
    general: &Path,
    external_node: &Path,
    genesis: &Path,
    contracts: &Path,
    secrets: &Path,
) -> anyhow::Result<()> {
    update_config(
        shell.clone(),
        general,
        &chain.path_to_general_config(),
        true,
        false,
        MSG_DIFF_GENERAL_CONFIG,
    )?;

    update_config(
        shell.clone(),
        external_node,
        &chain.path_to_external_node_config(),
        true,
        false,
        MSG_DIFF_EN_CONFIG,
    )?;

    update_config(
        shell.clone(),
        genesis,
        &chain.path_to_genesis_config(),
        false,
        false,
        &msg_diff_genesis_config(&chain.name),
    )?;

    update_config(
        shell.clone(),
        contracts,
        &chain.path_to_contracts_config(),
        false,
        false,
        &msg_diff_contracts_config(&chain.name),
    )?;

    update_config(
        shell.clone(),
        secrets,
        &chain.path_to_secrets_config(),
        false,
        true,
        &msg_diff_secrets(&chain.name, &chain.path_to_secrets_config(), secrets),
    )?;

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

#[cfg(test)]
mod tests {
    #[test]
    fn test_merge_yaml_both_are_equal_returns_no_diff() {
        let mut a = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            "#,
        )
        .unwrap();
        let b: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            "#,
        )
        .unwrap();
        let diff = super::merge_yaml(&mut a, b).unwrap();
        assert!(diff.value_diff.is_empty());
        assert!(diff.added_fields.is_empty());
    }

    #[test]
    fn test_merge_yaml_b_has_extra_field_returns_diff() {
        let mut a = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            "#,
        )
        .unwrap();
        let b: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            key5: value5
            "#,
        )
        .unwrap();
        let diff = super::merge_yaml(&mut a, b.clone()).unwrap();
        assert!(diff.value_diff.is_empty());
        assert_eq!(diff.added_fields.len(), 1);
        assert_eq!(
            diff.added_fields.get::<String>("key5".into()).unwrap(),
            b.clone().get("key5").unwrap()
        );
        assert_eq!(a.get("key5"), b.get("key5"));
    }

    #[test]
    fn test_merge_yaml_a_has_extra_field_no_diff() {
        let mut a = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            key5: value5
            "#,
        )
        .unwrap();
        let b: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            "#,
        )
        .unwrap();
        let diff = super::merge_yaml(&mut a, b).unwrap();
        assert!(diff.value_diff.is_empty());
        assert!(diff.added_fields.is_empty());
    }

    #[test]
    fn test_merge_yaml_a_has_extra_field_and_b_has_extra_field_returns_diff() {
        let mut a = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            key5: value5
            "#,
        )
        .unwrap();
        let b: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            key6: value6
            "#,
        )
        .unwrap();
        let diff = super::merge_yaml(&mut a, b.clone()).unwrap();
        assert_eq!(diff.value_diff.len(), 0);
        assert_eq!(diff.added_fields.len(), 1);
        assert_eq!(
            diff.added_fields.get::<String>("key6".into()).unwrap(),
            b.clone().get("key6").unwrap()
        );
        assert_eq!(a.get("key6"), b.get("key6"));
    }

    #[test]
    fn test_merge_yaml_a_has_different_value_returns_diff() {
        let mut a = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            "#,
        )
        .unwrap();
        let b: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value5
            "#,
        )
        .unwrap();
        let diff = super::merge_yaml(&mut a, b.clone()).unwrap();
        assert_eq!(diff.value_diff.len(), 1);
        assert_eq!(
            diff.value_diff
                .get::<serde_yaml::Value>("key4".into())
                .unwrap(),
            b.get("key3").unwrap().get("key4").unwrap()
        );
    }
}
