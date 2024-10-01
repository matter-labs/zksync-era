use anyhow::Context;
use common::logger;
use config::{
    copy_configs, set_l1_rpc_url, update_from_chain_config,
    ChainConfig, ContractsConfig, EcosystemConfig,
    traits::SaveConfigWithBasePath,
    DEFAULT_CONSENSUS_PORT,
};
use ethers::types::Address;
use xshell::Shell;

use crate::{
    commands::{
        chain::{
            args::init::configs::{InitConfigsArgs, InitConfigsArgsFinal},
            genesis,
        },
        portal::update_portal_config,
    },
    defaults::PORT_RANGE_END,
    messages::{
        MSG_CHAIN_CONFIGS_INITIALIZED, MSG_CHAIN_NOT_FOUND_ERR,
        MSG_PORTAL_FAILED_TO_CREATE_CONFIG_ERR,
    },
    utils::{
        consensus::{generate_consensus_keys, get_consensus_config, get_consensus_secrets},
        ports::EcosystemPortsScanner,
    },
};

pub async fn run(args: InitConfigsArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    let args = args.fill_values_with_prompt(&chain_config);

    init_configs(&args, shell, &ecosystem_config, &chain_config).await?;
    logger::outro(MSG_CHAIN_CONFIGS_INITIALIZED);

    Ok(())
}

pub async fn init_configs(
    init_args: &InitConfigsArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    chain_config: &ChainConfig,
) -> anyhow::Result<ContractsConfig> {
    // Port scanner should run before copying configs to avoid marking initial ports as assigned
    let mut ecosystem_ports = EcosystemPortsScanner::scan(shell)?;
    copy_configs(shell, &ecosystem_config.link_to_code, &chain_config.configs)?;

    if !init_args.no_port_reallocation {
        ecosystem_ports.allocate_ports_in_yaml(
            shell,
            &chain_config.path_to_general_config(),
            chain_config.id,
        )?;
    }

    // Initialize general config
    let mut general_config = chain_config.get_general_config()?;

    // TODO: This is a temporary solution. We should allocate consensus port using `EcosystemPorts::allocate_ports_in_yaml`
    let offset = ((chain_config.id - 1) * 100) as u16;
    let consensus_port_range = DEFAULT_CONSENSUS_PORT + offset..PORT_RANGE_END;
    let consensus_port =
        ecosystem_ports.allocate_port(consensus_port_range, "Consensus".to_string())?;

    let consensus_keys = generate_consensus_keys();
    let consensus_config = get_consensus_config(
        chain_config,
        consensus_port,
        Some(consensus_keys.clone()),
        None,
    )?;
    general_config.consensus_config = Some(consensus_config);
    general_config.save_with_base_path(shell, &chain_config.configs)?;

    // Initialize genesis config
    let mut genesis_config = chain_config.get_genesis_config()?;
    update_from_chain_config(&mut genesis_config, chain_config);
    genesis_config.save_with_base_path(shell, &chain_config.configs)?;

    // Initialize contracts config
    let mut contracts_config = ecosystem_config.get_contracts_config()?;
    contracts_config.l1.diamond_proxy_addr = Address::zero();
    contracts_config.l1.governance_addr = Address::zero();
    contracts_config.l1.chain_admin_addr = Address::zero();
    contracts_config.l1.base_token_addr = chain_config.base_token.address;
    contracts_config.save_with_base_path(shell, &chain_config.configs)?;

    // Initialize secrets config
    let mut secrets = chain_config.get_secrets_config()?;
    set_l1_rpc_url(&mut secrets, init_args.l1_rpc_url.clone())?;
    secrets.consensus = Some(get_consensus_secrets(&consensus_keys));
    secrets.save_with_base_path(shell, &chain_config.configs)?;

    genesis::database::update_configs(init_args.genesis_args.clone(), shell, chain_config)?;

    update_portal_config(shell, chain_config)
        .await
        .context(MSG_PORTAL_FAILED_TO_CREATE_CONFIG_ERR)?;

    Ok(contracts_config)
}
