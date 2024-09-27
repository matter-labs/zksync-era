use anyhow::{bail, Context};
use common::{config::global_config, logger};
use config::{
    copy_configs, ports_config, set_l1_rpc_url, traits::SaveConfigWithBasePath,
    update_from_chain_config, update_ports, ChainConfig, ContractsConfig, EcosystemConfig,
    GeneralConfig,
};
use ethers::types::Address;
use xshell::Shell;

use crate::{
    commands::{
        chain::{
            args::init::{
                configs::{InitConfigsArgs, InitConfigsArgsFinal},
                PortOffset,
            },
            genesis,
        },
        portal::update_portal_config,
    },
    messages::{
        MSG_CHAIN_CONFIGS_INITIALIZED, MSG_CHAIN_NOT_FOUND_ERR,
        MSG_PORTAL_FAILED_TO_CREATE_CONFIG_ERR, MSG_PORTS_CONFIG_ERR,
    },
    utils::consensus::{generate_consensus_keys, get_consensus_config, get_consensus_secrets},
};

pub async fn run(args: InitConfigsArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_name = global_config().chain_name.clone();
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_chain(chain_name)
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
    copy_configs(shell, &ecosystem_config.link_to_code, &chain_config.configs)?;

    // Initialize general config
    let mut general_config = chain_config.get_general_config()?;
    let port_offset = PortOffset::from_chain_id(chain_config.id as u16).into();
    apply_port_offset(port_offset, &mut general_config)?;
    let ports = ports_config(&general_config).context(MSG_PORTS_CONFIG_ERR)?;

    let consensus_keys = generate_consensus_keys();
    let consensus_config =
        get_consensus_config(chain_config, ports, Some(consensus_keys.clone()), None)?;
    general_config.consensus_config = Some(consensus_config);

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

    genesis::database::update_configs(init_args.genesis_args.clone(), shell, &chain_config)?;

    update_portal_config(shell, chain_config)
        .await
        .context(MSG_PORTAL_FAILED_TO_CREATE_CONFIG_ERR)?;

    Ok(contracts_config)
}

fn apply_port_offset(port_offset: u16, general_config: &mut GeneralConfig) -> anyhow::Result<()> {
    let Some(mut ports_config) = ports_config(general_config) else {
        bail!("Missing ports config");
    };

    ports_config.apply_offset(port_offset);

    update_ports(general_config, &ports_config)?;

    Ok(())
}
