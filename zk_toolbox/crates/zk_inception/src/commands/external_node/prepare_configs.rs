use std::path::Path;

use anyhow::Context;
use common::{config::global_config, logger};
use config::{
    external_node::ENConfig, traits::SaveConfigWithBasePath, ChainConfig, DatabaseSecrets,
    EcosystemConfig, L1Secret, SecretsConfig,
};
use xshell::Shell;

use crate::{
    commands::external_node::args::prepare_configs::{PrepareConfigArgs, PrepareConfigFinal},
    messages::{
        msg_preparing_en_config_is_done, MSG_CHAIN_NOT_INITIALIZED, MSG_PREPARING_EN_CONFIGS,
    },
    utils::rocks_db::{recreate_rocksdb_dirs, RocksDBDirOption},
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
    general_en.update_ports(&general.ports_config().next_empty_ports_config());
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
    let dirs = recreate_rocksdb_dirs(shell, &config.rocks_db_path, RocksDBDirOption::ExternalNode)?;
    general_en.set_rocks_db_config(dirs)?;

    general_en.save_with_base_path(shell, &en_configs_path)?;
    en_config.save_with_base_path(shell, &en_configs_path)?;

    Ok(())
}
