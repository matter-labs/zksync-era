use std::path::{Path, PathBuf};

use anyhow::Ok;
use common::{
    db::migrate_db,
    git, logger,
    spinner::Spinner,
    yaml::{merge_yaml, ConfigDiff},
};
use config::{
    zkstack_config::ZkStackConfig, ChainConfig, CONFIGS_PATH, CONTRACTS_FILE, EN_CONFIG_FILE,
    ERA_OBSERBAVILITY_DIR, GENERAL_FILE, GENESIS_FILE, SECRETS_FILE,
};
use xshell::Shell;

use super::args::UpdateArgs;
use crate::{
    consts::{PROVER_MIGRATIONS, SERVER_MIGRATIONS},
    messages::{
        msg_diff_contracts_config, msg_diff_genesis_config, msg_diff_secrets, msg_updating_chain,
        MSG_DIFF_EN_CONFIG, MSG_DIFF_EN_GENERAL_CONFIG, MSG_DIFF_GENERAL_CONFIG,
        MSG_PULLING_ZKSYNC_CODE_SPINNER, MSG_UPDATING_ERA_OBSERVABILITY_SPINNER,
        MSG_UPDATING_SUBMODULES_SPINNER, MSG_UPDATING_ZKSYNC, MSG_ZKSYNC_UPDATED,
    },
};

pub async fn run(shell: &Shell, args: UpdateArgs) -> anyhow::Result<()> {
    logger::info(MSG_UPDATING_ZKSYNC);
    let config = ZkStackConfig::from_file(shell)?;
    let link_to_code = config.link_to_code();
    let default_configs_path = link_to_code.join(CONFIGS_PATH);

    if !args.only_config {
        update_repo(shell, link_to_code)?;
        let path_to_era_observability = shell.current_dir().join(ERA_OBSERBAVILITY_DIR);
        if shell.path_exists(path_to_era_observability.clone()) {
            let spinner = Spinner::new(MSG_UPDATING_ERA_OBSERVABILITY_SPINNER);
            git::pull(shell, path_to_era_observability)?;
            spinner.finish();
        }
    }

    let general_config_path = default_configs_path.join(GENERAL_FILE);
    let external_node_config_path = default_configs_path.join(EN_CONFIG_FILE);
    let genesis_config_path = default_configs_path.join(GENESIS_FILE);
    let contracts_config_path = default_configs_path.join(CONTRACTS_FILE);
    let secrets_path = default_configs_path.join(SECRETS_FILE);

    let chains = match config {
        ZkStackConfig::EcosystemConfig(ecosystem) => {
            let mut chains = vec![];
            for chain_name in ecosystem.list_of_chains() {
                chains.push(ecosystem.load_chain(Some(chain_name))?);
            }
            chains
        }
        ZkStackConfig::ChainConfig(chain) => vec![chain],
    };

    for chain in chains {
        logger::step(msg_updating_chain(&chain.name));
        update_chain(
            shell,
            &chain,
            &general_config_path,
            &external_node_config_path,
            &genesis_config_path,
            &contracts_config_path,
            &secrets_path,
        )
        .await?;
    }

    logger::outro(MSG_ZKSYNC_UPDATED);

    Ok(())
}

fn update_repo(shell: &Shell, link_to_code: PathBuf) -> anyhow::Result<()> {
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
    let diff = merge_yaml(&mut chain_config, original_config, false)?;
    if save_config {
        save_updated_config(&shell, chain_config, chain_config_path, diff, msg)?;
    } else {
        diff.print(msg, true);
    }

    Ok(())
}

async fn update_chain(
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

    let secrets = chain.get_secrets_config()?;

    if let Some(db) = secrets.database {
        if let Some(url) = db.server_url {
            let path_to_migration = chain.link_to_code.join(SERVER_MIGRATIONS);
            migrate_db(shell, path_to_migration, url.expose_url()).await?;
        }
        if let Some(url) = db.prover_url {
            let path_to_migration = chain.link_to_code.join(PROVER_MIGRATIONS);
            migrate_db(shell, path_to_migration, url.expose_url()).await?;
        }
    }
    Ok(())
}
