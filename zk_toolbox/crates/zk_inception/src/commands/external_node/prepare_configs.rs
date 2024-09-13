use std::{collections::BTreeMap, path::Path, str::FromStr};

use anyhow::Context;
use common::{config::global_config, logger};
use config::{
    external_node::ENConfig, ports_config, set_rocks_db_config, traits::SaveConfigWithBasePath,
    update_ports, ChainConfig, EcosystemConfig, SecretsConfig,
};
use xshell::Shell;
use zksync_basic_types::url::SensitiveUrl;
use zksync_config::configs::{
    consensus::{ConsensusSecrets, NodeSecretKey, Secret},
    DatabaseSecrets, L1Secrets,
};
use zksync_consensus_crypto::TextFmt;
use zksync_consensus_roles as roles;

use crate::{
    commands::external_node::args::prepare_configs::{PrepareConfigArgs, PrepareConfigFinal},
    messages::{
        msg_preparing_en_config_is_done, MSG_CHAIN_NOT_INITIALIZED,
        MSG_CONSENSUS_CONFIG_MISSING_ERR, MSG_CONSENSUS_SECRETS_MISSING_ERR,
        MSG_CONSENSUS_SECRETS_NODE_KEY_MISSING_ERR, MSG_PORTS_CONFIG_ERR, MSG_PREPARING_EN_CONFIGS,
    },
    utils::{
        consensus::{get_consensus_config, node_public_key},
        rocks_db::{recreate_rocksdb_dirs, RocksDBDirOption},
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
        gateway_url: None,
    };
    let mut general_en = general.clone();
    let next_empty_ports_config = ports_config(&general)
        .context(MSG_PORTS_CONFIG_ERR)?
        .next_empty_ports_config();
    update_ports(&mut general_en, &next_empty_ports_config)?;

    // Set consensus config
    let main_node_consensus_config = general
        .consensus_config
        .context(MSG_CONSENSUS_CONFIG_MISSING_ERR)?;

    let mut gossip_static_outbound = BTreeMap::new();
    let main_node_public_key = node_public_key(
        &config
            .get_secrets_config()?
            .consensus
            .context(MSG_CONSENSUS_SECRETS_MISSING_ERR)?,
    )?
    .context(MSG_CONSENSUS_SECRETS_NODE_KEY_MISSING_ERR)?;

    gossip_static_outbound.insert(main_node_public_key, main_node_consensus_config.public_addr);

    let en_consensus_config = get_consensus_config(
        config,
        next_empty_ports_config,
        None,
        Some(gossip_static_outbound),
    )?;
    general_en.consensus_config = Some(en_consensus_config.clone());
    en_consensus_config.save_with_base_path(shell, en_configs_path)?;

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
        }),
    };
    secrets.save_with_base_path(shell, en_configs_path)?;
    let dirs = recreate_rocksdb_dirs(shell, &config.rocks_db_path, RocksDBDirOption::ExternalNode)?;
    set_rocks_db_config(&mut general_en, dirs)?;
    general_en.save_with_base_path(shell, en_configs_path)?;
    en_config.save_with_base_path(shell, en_configs_path)?;

    Ok(())
}
