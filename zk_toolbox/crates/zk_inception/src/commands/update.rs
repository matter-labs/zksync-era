use std::path::Path;

use anyhow::{Context, Ok};
use common::{git, logger, spinner::Spinner};
use config::{
    ChainConfig, EcosystemConfig, CONTRACTS_FILE, EN_CONFIG_FILE, GENERAL_FILE, GENESIS_FILE,
    SECRETS_FILE,
};
use xshell::Shell;

use super::args::UpdateArgs;
use crate::messages::{
    msg_diff_contracts_config, msg_diff_genesis_config, msg_diff_secrets, msg_updating_chain,
    MSG_CHAIN_NOT_FOUND_ERR, MSG_DIFF_EN_CONFIG, MSG_DIFF_EN_GENERAL_CONFIG,
    MSG_DIFF_GENERAL_CONFIG, MSG_INVALID_KEY_TYPE_ERR, MSG_PULLING_ZKSYNC_CODE_SPINNER,
    MSG_UPDATING_ERA_OBSERVABILITY_SPINNER, MSG_UPDATING_SUBMODULES_SPINNER, MSG_UPDATING_ZKSYNC,
    MSG_ZKSYNC_UPDATED,
};

/// Holds the differences between two YAML configurations.
#[derive(Default)]
struct ConfigDiff {
    /// Fields that have different values between the two configurations
    /// This contains the new values
    pub differing_values: serde_yaml::Mapping,

    /// Fields that are present in the new configuration but not in the old one.
    pub new_fields: serde_yaml::Mapping,
}

impl ConfigDiff {
    fn print(&self, msg: &str, is_warning: bool) {
        if self.new_fields.is_empty() {
            return;
        }

        if is_warning {
            logger::warn(msg);
            logger::warn(logger::object_to_string(&self.new_fields));
        } else {
            logger::info(msg);
            logger::info(logger::object_to_string(&self.new_fields));
        }
    }
}

pub fn run(shell: &Shell, args: UpdateArgs) -> anyhow::Result<()> {
    logger::info(MSG_UPDATING_ZKSYNC);
    let ecosystem = EcosystemConfig::from_file(shell)?;

    if !args.only_config {
        update_repo(shell, &ecosystem)?;
    }

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

    let path_to_era_observability = shell.current_dir().join("era-observability");
    if shell.path_exists(path_to_era_observability.clone()) {
        let spinner = Spinner::new(MSG_UPDATING_ERA_OBSERVABILITY_SPINNER);
        git::pull(shell, path_to_era_observability)?;
        spinner.finish();
    }

    logger::outro(MSG_ZKSYNC_UPDATED);

    Ok(())
}

fn update_repo(shell: &Shell, ecosystem: &EcosystemConfig) -> anyhow::Result<()> {
    let link_to_code = ecosystem.link_to_code.clone();

    let spinner = Spinner::new(MSG_PULLING_ZKSYNC_CODE_SPINNER);
    git::pull(shell, link_to_code.clone())?;
    spinner.finish();
    let spinner = Spinner::new(MSG_UPDATING_SUBMODULES_SPINNER);
    git::submodule_update(shell, link_to_code.clone())?;
    spinner.finish();

    Ok(())
}

fn save_updated_config(
    shell: &Shell,
    config: serde_yaml::Value,
    path: &Path,
    diff: ConfigDiff,
    msg: &str,
) -> anyhow::Result<()> {
    if diff.new_fields.is_empty() {
        return Ok(());
    }

    diff.print(msg, false);

    let general_config = serde_yaml::to_string(&config)?;
    shell.write_file(path, general_config)?;

    Ok(())
}

fn update_config(
    shell: Shell,
    original_config_path: &Path,
    chain_config_path: &Path,
    save_config: bool,
    msg: &str,
) -> anyhow::Result<()> {
    let original_config = serde_yaml::from_str(&shell.read_file(original_config_path)?)?;
    let mut chain_config = serde_yaml::from_str(&shell.read_file(chain_config_path)?)?;
    let diff = merge_yaml(&mut chain_config, original_config)?;
    if save_config {
        save_updated_config(&shell, chain_config, chain_config_path, diff, msg)?;
    } else {
        diff.print(msg, true);
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
        MSG_DIFF_GENERAL_CONFIG,
    )?;

    update_config(
        shell.clone(),
        external_node,
        &chain.path_to_external_node_config(),
        true,
        MSG_DIFF_EN_CONFIG,
    )?;

    update_config(
        shell.clone(),
        genesis,
        &chain.path_to_genesis_config(),
        false,
        &msg_diff_genesis_config(&chain.name),
    )?;

    update_config(
        shell.clone(),
        contracts,
        &chain.path_to_contracts_config(),
        false,
        &msg_diff_contracts_config(&chain.name),
    )?;

    update_config(
        shell.clone(),
        secrets,
        &chain.path_to_secrets_config(),
        false,
        &msg_diff_secrets(&chain.name, &chain.path_to_secrets_config(), secrets),
    )?;

    if let Some(external_node_config_path) = chain.external_node_config_path.clone() {
        let external_node_general_config_path = external_node_config_path.join(GENERAL_FILE);
        if !shell.path_exists(external_node_general_config_path.clone()) {
            return Ok(());
        }
        update_config(
            shell.clone(),
            general,
            &external_node_general_config_path,
            true,
            MSG_DIFF_EN_GENERAL_CONFIG,
        )?;
    }

    Ok(())
}

fn merge_yaml_internal(
    a: &mut serde_yaml::Value,
    b: serde_yaml::Value,
    current_key: String,
    diff: &mut ConfigDiff,
) -> anyhow::Result<()> {
    match (a, b) {
        (serde_yaml::Value::Mapping(a), serde_yaml::Value::Mapping(b)) => {
            for (key, value) in b {
                let k = key.as_str().context(MSG_INVALID_KEY_TYPE_ERR)?.to_string();
                let current_key = if current_key.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", current_key, k)
                };

                if a.contains_key(&key) {
                    merge_yaml_internal(a.get_mut(&key).unwrap(), value, current_key, diff)?;
                } else {
                    a.insert(key.clone(), value.clone());
                    diff.new_fields.insert(current_key.into(), value);
                }
            }
        }
        (a, b) => {
            if a != &b {
                diff.differing_values.insert(current_key.into(), b);
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
        let expected: serde_yaml::Value = serde_yaml::from_str(
            r#"
        key1: value1
        key2: value2
        key3:
            key4: value4
        "#,
        )
        .unwrap();
        let diff = super::merge_yaml(&mut a, b).unwrap();
        assert!(diff.differing_values.is_empty());
        assert!(diff.new_fields.is_empty());
        assert_eq!(a, expected);
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

        let expected: serde_yaml::Value = serde_yaml::from_str(
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
        assert!(diff.differing_values.is_empty());
        assert_eq!(diff.new_fields.len(), 1);
        assert_eq!(
            diff.new_fields.get::<String>("key5".into()).unwrap(),
            b.clone().get("key5").unwrap()
        );
        assert_eq!(a, expected);
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

        let expected: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            key5: value5
            "#,
        )
        .unwrap();

        let diff = super::merge_yaml(&mut a, b).unwrap();
        assert!(diff.differing_values.is_empty());
        assert!(diff.new_fields.is_empty());
        assert_eq!(a, expected);
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

        let expected: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            key5: value5
            key6: value6
            "#,
        )
        .unwrap();

        let diff = super::merge_yaml(&mut a, b.clone()).unwrap();
        assert_eq!(diff.differing_values.len(), 0);
        assert_eq!(diff.new_fields.len(), 1);
        assert_eq!(
            diff.new_fields.get::<String>("key6".into()).unwrap(),
            b.clone().get("key6").unwrap()
        );
        assert_eq!(a, expected);
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

        let expected: serde_yaml::Value = serde_yaml::from_str(
            r#"
            key1: value1
            key2: value2
            key3:
                key4: value4
            "#,
        )
        .unwrap();

        let diff = super::merge_yaml(&mut a, b.clone()).unwrap();
        assert_eq!(diff.differing_values.len(), 1);
        assert_eq!(
            diff.differing_values
                .get::<serde_yaml::Value>("key3.key4".into())
                .unwrap(),
            b.get("key3").unwrap().get("key4").unwrap()
        );
        assert_eq!(a, expected);
    }

    #[test]
    fn test_merge_yaml_a_has_different_value_and_b_has_extra_field_returns_diff() {
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
            key5: value5
            "#,
        )
        .unwrap();

        let expected: serde_yaml::Value = serde_yaml::from_str(
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
        assert_eq!(diff.differing_values.len(), 1);
        assert_eq!(
            diff.differing_values
                .get::<serde_yaml::Value>("key3.key4".into())
                .unwrap(),
            b.get("key3").unwrap().get("key4").unwrap()
        );
        assert_eq!(diff.new_fields.len(), 1);
        assert_eq!(
            diff.new_fields.get::<String>("key5".into()).unwrap(),
            b.get("key5").unwrap()
        );
        assert_eq!(a, expected);
    }
}
