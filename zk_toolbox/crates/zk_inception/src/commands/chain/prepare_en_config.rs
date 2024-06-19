use std::path::Path;

use anyhow::Context;
use common::{config::global_config, logger};
use config::{
    external_node::ENConfig, traits::SaveConfigWithBasePath, ChainConfig, DatabaseSecrets,
    EcosystemConfig, L1Secret, SecretsConfig,
};
use xshell::Shell;

use crate::{
    commands::chain::args::prepare_external_node_configs::{PrepareConfigArgs, PrepareConfigFinal},
    config_manipulations::update_rocks_db_config,
    defaults::EN_ROCKS_DB_PREFIX,
    messages::{
        msg_preparing_en_config_is_done, MSG_CHAIN_NOT_INITIALIZED, MSG_PREPARING_EN_CONFIGS,
    },
};

pub fn run(shell: &Shell, args: PrepareConfigArgs) -> anyhow::Result<()> {
    logger::info(MSG_PREPARING_EN_CONFIGS);
    let chain_name = global_config().chain_name.clone();
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let mut chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    let args = args.fill_values_with_prompt(&chain_config);
    let external_node_config_path = chain_config
        .external_node_config_path
        .unwrap_or_else(|| chain_config.configs.join("external_node"));
    shell.create_dir(&external_node_config_path)?;
    chain_config.external_node_config_path = Some(external_node_config_path.clone());
    prepare_configs(shell, &chain_config, &external_node_config_path, args)?;
    let chain_path = ecosystem_config.chains.join(&chain_config.name);
    chain_config.save_with_base_path(shell, chain_path)?;
    logger::info(msg_preparing_en_config_is_done(&external_node_config_path));
    Ok(())
}

fn prepare_configs(
    shell: &Shell,
    config: &ChainConfig,
    en_configs_path: &Path,
    args: PrepareConfigFinal,
) -> anyhow::Result<()> {
    let genesis = config.get_genesis_config()?;
    let general = config.get_general_config()?;
    let en_config = ENConfig {
        l2_chain_id: genesis.l2_chain_id,
        l1_chain_id: genesis.l1_chain_id,
        l1_batch_commit_data_generator_mode: genesis
            .l1_batch_commit_data_generator_mode
            .unwrap_or_default(),
        main_node_url: general.api.web3_json_rpc.http_url.clone(),
        main_node_rate_limit_rps: None,
    };
    let mut general_en = general.clone();
    general_en.api.web3_json_rpc.http_port = general_en.api.web3_json_rpc.http_port + 1000;
    general_en.api.web3_json_rpc.ws_port = general_en.api.web3_json_rpc.ws_port + 1000;
    general_en.api.healthcheck.port = general_en.api.healthcheck.port + 1000;
    general_en.api.merkle_tree.port = general_en.api.merkle_tree.port + 1000;
    general_en.api.prometheus.listener_port = general_en.api.prometheus.listener_port + 1000;
    let secrets = SecretsConfig {
        database: DatabaseSecrets {
            server_url: args.db.full_url(),
            prover_url: None,
            other: Default::default(),
        },
        l1: L1Secret {
            l1_rpc_url: args.l1_rpc_url.clone(),
            other: Default::default(),
        },
        other: Default::default(),
    };
    secrets.save_with_base_path(shell, en_configs_path)?;
    general_en.save_with_base_path(shell, &en_configs_path)?;
    update_rocks_db_config(
        shell,
        &en_configs_path,
        &config.rocks_db_path,
        EN_ROCKS_DB_PREFIX,
    )?;
    en_config.save_with_base_path(shell, &en_configs_path)?;

    Ok(())
}
