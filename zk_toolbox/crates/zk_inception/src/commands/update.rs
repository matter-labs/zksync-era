use std::path::Path;

use anyhow::Context;
use common::{
    git::{pull, submodule_update},
    logger,
    spinner::Spinner,
};
use config::EcosystemConfig;
use xshell::Shell;

use crate::{
    consts::GENERAL_FILE,
    messages::{
        MSG_CHAIN_NOT_FOUND_ERR, MSG_PULLING_ZKSYNC_CODE_SPINNER, MSG_SHOW_DIFF,
        MSG_UPDATING_GENERAL_CONFIG, MSG_UPDATING_SUBMODULES_SPINNER, MSG_UPDATING_ZKSYNC,
        MSG_ZKSYNC_UPDATED,
    },
};

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

    logger::info(MSG_UPDATING_GENERAL_CONFIG);
    let updated_config_path = ecosystem.get_default_configs_path().join(GENERAL_FILE);
    let updated_config = serde_yaml::from_reader(std::fs::File::open(updated_config_path)?)?;

    let current_config_path = ecosystem
        .load_chain(Some(ecosystem.default_chain.clone()))
        .context(MSG_CHAIN_NOT_FOUND_ERR)?
        .path_to_general_config();
    let mut current_config =
        serde_yaml::from_reader(std::fs::File::open(current_config_path.clone())?)?;

    let mut diff = serde_yaml::Mapping::new();

    merge_yaml(
        &mut current_config,
        updated_config,
        "".into(),
        &mut diff,
        false,
    )?;

    save_updated_config(current_config, &current_config_path, diff)?;

    logger::outro(MSG_ZKSYNC_UPDATED);

    Ok(())
}

fn merge_yaml(
    a: &mut serde_yaml::Value,
    b: serde_yaml::Value,
    current_key: serde_yaml::Value,
    diff: &mut serde_yaml::Mapping,
    overwrite: bool,
) -> anyhow::Result<()> {
    match (a, b) {
        (serde_yaml::Value::Mapping(a), serde_yaml::Value::Mapping(b)) => {
            for (key, value) in b {
                if a.contains_key(&key) {
                    merge_yaml(a.get_mut(&key).unwrap(), value, key, diff, overwrite)?;
                } else {
                    a.insert(key.clone(), value.clone());
                    diff.insert(key, value);
                }
            }
        }
        (a, b) => {
            if overwrite {
                *a = b.clone();
                diff.insert(current_key, b);
            }
        }
    }
    Ok(())
}

fn save_updated_config(
    config: serde_yaml::Value,
    path: &Path,
    diff: serde_yaml::Mapping,
) -> anyhow::Result<()> {
    if diff.is_empty() {
        return Ok(());
    }

    logger::info(MSG_SHOW_DIFF);
    for (key, value) in diff {
        let key = key.as_str().unwrap();
        logger::info(format!("{}: {:?}", key, value));
    }

    let general_config = serde_yaml::to_string(&config)?;
    std::fs::write(path, general_config)?;

    Ok(())
}
