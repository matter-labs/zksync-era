use std::{collections::BTreeMap, path::Path, str::FromStr};

use anyhow::Context;
use common::logger;
use config::{
    external_node::ENConfig,
    set_rocks_db_config,
    traits::{FileConfigWithDefaultName, SaveConfigWithBasePath},
    ChainConfig, EcosystemConfig, GeneralConfig, SecretsConfig,
};
use xshell::Shell;
use zksync_basic_types::url::SensitiveUrl;
use zksync_config::configs::{
    consensus::{ConsensusConfig, ConsensusSecrets, NodeSecretKey, Secret},
    DatabaseSecrets, L1Secrets,
};
use zksync_consensus_crypto::TextFmt;
use zksync_consensus_roles as roles;

use crate::{
    commands::external_node::args::prepare_configs::{PrepareConfigArgs, PrepareConfigFinal},
    messages::{
        msg_preparing_en_config_is_done, MSG_CHAIN_NOT_INITIALIZED,
        MSG_CONSENSUS_CONFIG_MISSING_ERR, MSG_CONSENSUS_SECRETS_MISSING_ERR,
        MSG_CONSENSUS_SECRETS_NODE_KEY_MISSING_ERR, MSG_PREPARING_EN_CONFIGS,
    },
    utils::{
        consensus::node_public_key,
        ports::EcosystemPortsScanner,
        rocks_db::{recreate_rocksdb_dirs, RocksDBDirOption},
    },
};

pub fn run(shell: &Shell, args: PrepareConfigArgs) -> anyhow::Result<()> {
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
    let mut ports = EcosystemPortsScanner::scan(shell)?;
    let genesis = config.get_genesis_config()?;
    let general = config.get_general_config()?;
    let en_config = ENConfig {
        l2_chain_id: genesis.l2_chain_id,
        l1_chain_id: genesis.l1_chain_id,
        sl_chain_id: genesis.sl_chain_id,
        l1_batch_commit_data_generator_mode: genesis.l1_batch_commit_data_generator_mode,
        main_node_url: SensitiveUrl::from_str(
            &general
                .api_config
                .as_ref()
                .context("api_config")?
                .web3_json_rpc
                .http_url,
        )?,
        main_node_rate_limit_rps: None,
        bridge_addresses_refresh_interval_sec: None,
    };
    let mut general_en = general.clone();
    general_en.consensus_config = None;

    let main_node_consensus_config = general
        .consensus_config
        .context(MSG_CONSENSUS_CONFIG_MISSING_ERR)?;
    let mut en_consensus_config = main_node_consensus_config.clone();

    let mut gossip_static_outbound = BTreeMap::new();
    let main_node_public_key = node_public_key(
        &config
            .get_secrets_config()?
            .consensus
            .context(MSG_CONSENSUS_SECRETS_MISSING_ERR)?,
    )?
    .context(MSG_CONSENSUS_SECRETS_NODE_KEY_MISSING_ERR)?;
    gossip_static_outbound.insert(main_node_public_key, main_node_consensus_config.public_addr);
    en_consensus_config.gossip_static_outbound = gossip_static_outbound;

    // Set secrets config
    let node_key = roles::node::SecretKey::generate().encode();
    let consensus_secrets = ConsensusSecrets {
        validator_key: None,
        attester_key: None,
        node_key: Some(NodeSecretKey(Secret::new(node_key))),
    };
    let secrets = SecretsConfig {
        consensus: Some(consensus_secrets),
        database: Some(DatabaseSecrets {
            server_url: Some(args.db.full_url().into()),
            prover_url: None,
            server_replica_url: None,
        }),
        l1: Some(L1Secrets {
            l1_rpc_url: SensitiveUrl::from_str(&args.l1_rpc_url).context("l1_rpc_url")?,
            gateway_rpc_url: None,
        }),
        data_availability: None,
    };

    let dirs = recreate_rocksdb_dirs(shell, &config.rocks_db_path, RocksDBDirOption::ExternalNode)?;
    set_rocks_db_config(&mut general_en, dirs)?;

    general_en.save_with_base_path(shell, en_configs_path)?;
    en_config.save_with_base_path(shell, en_configs_path)?;
    en_consensus_config.save_with_base_path(shell, en_configs_path)?;
    secrets.save_with_base_path(shell, en_configs_path)?;

    let offset = 0; // This is zero because general_en ports already have a chain offset
    ports.allocate_ports_in_yaml(
        shell,
        &GeneralConfig::get_path_with_base_path(en_configs_path),
        offset,
    )?;
    ports.allocate_ports_in_yaml(
        shell,
        &ConsensusConfig::get_path_with_base_path(en_configs_path),
        offset,
    )?;

    Ok(())
}
