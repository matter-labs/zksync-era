use std::path::Path;

use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{
    db::migrate_db,
    git, logger,
    spinner::Spinner,
    yaml::{merge_yaml, ConfigDiff},
};
use zkstack_cli_config::{
    ChainConfig, EcosystemConfig, ZkStackConfig, ZkStackConfigTrait, CONTRACTS_FILE,
    EN_CONFIG_FILE, ERA_OBSERBAVILITY_DIR, GENERAL_FILE, GENESIS_FILE, SECRETS_FILE,
};

use super::args::UpdateArgs;
use crate::{
    consts::{PROVER_MIGRATIONS, SERVER_MIGRATIONS},
    messages::{
        msg_diff_contracts_config, msg_diff_genesis_config, msg_diff_secrets, msg_updating_chain,
        MSG_CHAIN_NOT_FOUND_ERR, MSG_DIFF_EN_CONFIG, MSG_DIFF_EN_GENERAL_CONFIG,
        MSG_DIFF_GENERAL_CONFIG, MSG_PULLING_ZKSYNC_CODE_SPINNER,
        MSG_UPDATING_ERA_OBSERVABILITY_SPINNER, MSG_UPDATING_SUBMODULES_SPINNER,
        MSG_UPDATING_ZKSYNC, MSG_ZKSYNC_UPDATED,
    },
};

pub async fn run(shell: &Shell, args: UpdateArgs) -> anyhow::Result<()> {
    logger::info(MSG_UPDATING_ZKSYNC);
    let ecosystem = ZkStackConfig::ecosystem(shell)?;

    if !args.only_config {
        update_repo(shell, &ecosystem)?;
    }

    let general_config_path = ecosystem.default_configs_path().join(GENERAL_FILE);
    let external_node_config_path = ecosystem.default_configs_path().join(EN_CONFIG_FILE);
    let genesis_config_path = ecosystem.default_configs_path().join(GENESIS_FILE);
    let contracts_config_path = ecosystem.default_configs_path().join(CONTRACTS_FILE);
    let secrets_path = ecosystem.default_configs_path().join(SECRETS_FILE);

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
        )
        .await?;
    }

    let path_to_era_observability = shell.current_dir().join(ERA_OBSERBAVILITY_DIR);
    if shell.path_exists(path_to_era_observability.clone()) {
        let spinner = Spinner::new(MSG_UPDATING_ERA_OBSERVABILITY_SPINNER);
        git::pull(shell, &path_to_era_observability)?;
        spinner.finish();
    }

    logger::outro(MSG_ZKSYNC_UPDATED);

    Ok(())
}

fn update_repo(shell: &Shell, ecosystem: &EcosystemConfig) -> anyhow::Result<()> {
    let link_to_code = &ecosystem.link_to_code();

    let spinner = Spinner::new(MSG_PULLING_ZKSYNC_CODE_SPINNER);
    git::pull(shell, link_to_code)?;
    spinner.finish();
    let spinner = Spinner::new(MSG_UPDATING_SUBMODULES_SPINNER);
    git::submodule_update(shell, link_to_code)?;
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

    let secrets = chain.get_secrets_config().await?;
    if let Some(url) = secrets.core_database_url()? {
        let path_to_migration = chain.link_to_code().join(SERVER_MIGRATIONS);
        migrate_db(shell, &path_to_migration, &url).await?;
    }
    if let Some(url) = secrets.prover_database_url()? {
        let path_to_migration = chain.link_to_code().join(PROVER_MIGRATIONS);
        migrate_db(shell, &path_to_migration, &url).await?;
    }
    Ok(())
}
