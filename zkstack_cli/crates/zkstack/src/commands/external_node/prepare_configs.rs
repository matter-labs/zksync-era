use std::path::Path;

use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::logger;
use zkstack_cli_config::{
    raw::{PatchedConfig, RawConfig},
    set_rocks_db_config, ChainConfig, EcosystemConfig, CONSENSUS_CONFIG_FILE, EN_CONFIG_FILE,
    GENERAL_FILE, SECRETS_FILE,
};
use zksync_basic_types::{L1ChainId, L2ChainId};
use zksync_consensus_crypto::TextFmt;
use zksync_consensus_roles as roles;

use crate::{
    commands::external_node::args::prepare_configs::{PrepareConfigArgs, PrepareConfigFinal},
    messages::{
        msg_preparing_en_config_is_done, MSG_CHAIN_NOT_INITIALIZED, MSG_PREPARING_EN_CONFIGS,
    },
    utils::{
        consensus::{node_public_key, KeyAndAddress},
        ports::EcosystemPortsScanner,
        rocks_db::{recreate_rocksdb_dirs, RocksDBDirOption},
    },
};

pub async fn run(shell: &Shell, args: PrepareConfigArgs) -> anyhow::Result<()> {
    logger::info(MSG_PREPARING_EN_CONFIGS);
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let mut chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    let args = args.fill_values_with_prompt(&chain_config);
    let external_node_config_path = chain_config
        .external_node_config_path
        .unwrap_or_else(|| chain_config.configs.join("external_node"));
    shell.create_dir(&external_node_config_path)?;
    chain_config.external_node_config_path = Some(external_node_config_path.clone());
    prepare_configs(shell, &chain_config, &external_node_config_path, args).await?;
    let chain_path = ecosystem_config.chains.join(&chain_config.name);
    chain_config.save_with_base_path(shell, chain_path)?;
    logger::info(msg_preparing_en_config_is_done(&external_node_config_path));
    Ok(())
}

async fn prepare_configs(
    shell: &Shell,
    config: &ChainConfig,
    en_configs_path: &Path,
    args: PrepareConfigFinal,
) -> anyhow::Result<()> {
    // Reallocating ports for en required to use the current chain ports as well
    let mut ports = EcosystemPortsScanner::scan(shell, None)?;
    let genesis = config.get_genesis_config().await?;
    let general = config.get_general_config().await?;
    let gateway = config.get_gateway_chain_config().ok();
    let l2_rpc_port = general.get::<u16>("api.web3_json_rpc.http_port")?;

    let mut en_config = PatchedConfig::empty(shell, en_configs_path.join(EN_CONFIG_FILE));
    en_config.insert(
        "l2_chain_id",
        genesis.get::<L2ChainId>("l2_chain_id")?.as_u64(),
    )?;
    en_config.insert("l1_chain_id", genesis.get::<L1ChainId>("l1_chain_id")?.0)?;
    en_config.insert_yaml(
        "l1_batch_commit_data_generator_mode",
        genesis.get::<String>("l1_batch_commit_data_generator_mode")?,
    )?;
    en_config.insert("main_node_url", format!("http://127.0.0.1:{l2_rpc_port}"))?;
    if let Some(gateway) = &gateway {
        en_config.insert_yaml("gateway_chain_id", gateway.gateway_chain_id)?;
    }
    en_config.save().await?;

    // Copy and modify the general config
    let general_config_path = en_configs_path.join(GENERAL_FILE);
    shell.copy_file(config.path_to_general_config(), &general_config_path)?;
    let mut general_en = RawConfig::read(shell, general_config_path.clone())
        .await?
        .patched();
    let main_node_public_addr: String = general_en.base().get("consensus.public_addr")?;
    let raw_consensus = general_en.base().get("consensus")?;
    general_en.remove("consensus");

    // Copy and modify the consensus config
    let mut en_consensus_config =
        PatchedConfig::empty(shell, en_configs_path.join(CONSENSUS_CONFIG_FILE));
    en_consensus_config.extend(raw_consensus);
    let main_node_public_key = node_public_key(
        &config
            .get_secrets_config()
            .await?
            .get::<String>("consensus.node_key")?,
    )?;
    let gossip_static_outbound = [KeyAndAddress {
        key: main_node_public_key,
        addr: main_node_public_addr,
    }];
    en_consensus_config.insert_yaml("gossip_static_outbound", gossip_static_outbound)?;
    en_consensus_config.save().await?;

    // Set secrets config
    let mut secrets = PatchedConfig::empty(shell, en_configs_path.join(SECRETS_FILE));
    let node_key = roles::node::SecretKey::generate().encode();
    secrets.insert("consensus.node_key", node_key)?;
    secrets.insert("database.server_url", args.db.full_url().to_string())?;
    secrets.insert("l1.l1_rpc_url", args.l1_rpc_url)?;
    if let Some(url) = args.gateway_rpc_url {
        secrets.insert("l1.gateway_rpc_url", url)?;
    }
    secrets.save().await?;

    let dirs = recreate_rocksdb_dirs(shell, &config.rocks_db_path, RocksDBDirOption::ExternalNode)?;
    set_rocks_db_config(&mut general_en, dirs)?;
    general_en.save().await?;

    let offset = 0; // This is zero because general_en ports already have a chain offset
    ports.allocate_ports_in_yaml(shell, &general_config_path, offset)?;
    ports.allocate_ports_in_yaml(shell, &en_configs_path.join(CONSENSUS_CONFIG_FILE), offset)?;

    Ok(())
}
