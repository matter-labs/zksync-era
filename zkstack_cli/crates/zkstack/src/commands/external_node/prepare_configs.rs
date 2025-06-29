use std::path::Path;

use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::logger;
use zkstack_cli_config::{
    ChainConfig, EcosystemConfig, ExternalNodeConfigPatch, GatewayChainConfig, GeneralConfig,
    KeyAndAddress, SecretsConfigPatch, CONSENSUS_CONFIG_FILE, EN_CONFIG_FILE, GENERAL_FILE,
    SECRETS_FILE,
};
use zksync_consensus_crypto::TextFmt;
use zksync_consensus_roles as roles;

use crate::{
    commands::external_node::args::prepare_configs::{PrepareConfigArgs, PrepareConfigFinal},
    messages::{
        msg_preparing_en_config_is_done, MSG_CHAIN_NOT_INITIALIZED, MSG_PREPARING_EN_CONFIGS,
    },
    utils::{
        consensus::node_public_key,
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
    let gateway = config.get_gateway_chain_config().await.ok();
    let l2_rpc_url = general.l2_http_url()?;

    let mut en_config = ExternalNodeConfigPatch::empty(shell, en_configs_path.join(EN_CONFIG_FILE));
    en_config.set_chain_ids(
        genesis.l1_chain_id()?,
        genesis.l2_chain_id()?,
        gateway
            .as_ref()
            .map(GatewayChainConfig::gateway_chain_id)
            .transpose()?,
    )?;
    en_config.set_main_node_url(&l2_rpc_url)?;
    en_config.save().await?;

    // Copy and modify the general config
    let general_config_path = en_configs_path.join(GENERAL_FILE);
    shell.copy_file(config.path_to_general_config(), &general_config_path)?;
    let general_en = GeneralConfig::read(shell, general_config_path.clone()).await?;
    let main_node_public_addr = general_en.consensus_public_addr()?;
    let mut general_en = general_en.patched();

    // Extract and modify the consensus config
    let mut en_consensus_config =
        general_en.extract_consensus(shell, en_configs_path.join(CONSENSUS_CONFIG_FILE))?;
    let main_node_public_key = node_public_key(
        &config
            .get_secrets_config()
            .await?
            .raw_consensus_node_key()?,
    )?;
    let gossip_static_outbound = vec![KeyAndAddress {
        key: main_node_public_key,
        addr: main_node_public_addr,
    }];
    en_consensus_config.set_static_outbound_peers(gossip_static_outbound)?;
    en_consensus_config.save().await?;

    // Set secrets config
    let mut secrets = SecretsConfigPatch::empty(shell, en_configs_path.join(SECRETS_FILE));
    let node_key = roles::node::SecretKey::generate().encode();
    secrets.set_consensus_node_key(&node_key)?;
    secrets.set_server_database(&args.db)?;
    secrets.set_l1_rpc_url(args.l1_rpc_url)?;
    if let Some(url) = args.gateway_rpc_url {
        secrets.set_gateway_rpc_url(url)?;
    }
    secrets.save().await?;

    let dirs = recreate_rocksdb_dirs(shell, &config.rocks_db_path, RocksDBDirOption::ExternalNode)?;
    general_en.set_rocks_db_config(dirs)?;
    general_en.save().await?;

    let offset = 0; // This is zero because general_en ports already have a chain offset
    ports.allocate_ports_in_yaml(shell, &general_config_path, offset, args.tight_ports)?;
    ports.allocate_ports_in_yaml(shell, &en_configs_path.join(CONSENSUS_CONFIG_FILE), offset, args.tight_ports)?;

    Ok(())
}
