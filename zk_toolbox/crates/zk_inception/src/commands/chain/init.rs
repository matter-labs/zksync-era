use anyhow::{bail, Context};
use common::{config::global_config, git, logger, spinner::Spinner};
use config::{
    copy_configs, ports_config, set_l1_rpc_url, traits::SaveConfigWithBasePath,
    update_from_chain_config, update_ports, ChainConfig, EcosystemConfig, GeneralConfig,
};
use types::BaseToken;
use xshell::Shell;

use super::common::{distribute_eth, mint_base_token, register_chain};
use crate::{
    accept_ownership::accept_admin,
    commands::{
        chain::{
            args::init::{InitArgs, InitArgsFinal},
            deploy_l2_contracts, deploy_paymaster,
            genesis::genesis,
            set_token_multiplier_setter::set_token_multiplier_setter,
            setup_legacy_bridge::setup_legacy_bridge,
        },
        portal::update_portal_config,
    },
    messages::{
        msg_initializing_chain, MSG_ACCEPTING_ADMIN_SPINNER, MSG_CHAIN_INITIALIZED,
        MSG_CHAIN_NOT_FOUND_ERR, MSG_DEPLOYING_PAYMASTER, MSG_GENESIS_DATABASE_ERR,
        MSG_PORTAL_FAILED_TO_CREATE_CONFIG_ERR, MSG_PORTS_CONFIG_ERR,
        MSG_REGISTERING_CHAIN_SPINNER, MSG_SELECTED_CONFIG,
        MSG_UPDATING_TOKEN_MULTIPLIER_SETTER_SPINNER, MSG_WALLET_TOKEN_MULTIPLIER_SETTER_NOT_FOUND,
    },
    utils::consensus::{generate_consensus_keys, get_consensus_config, get_consensus_secrets},
};

pub(crate) async fn run(args: InitArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_name = global_config().chain_name.clone();
    let config = EcosystemConfig::from_file(shell)?;
    let chain_config = config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    let mut args = args.fill_values_with_prompt(&chain_config);

    logger::note(MSG_SELECTED_CONFIG, logger::object_to_string(&chain_config));
    logger::info(msg_initializing_chain(""));
    git::submodule_update(shell, config.link_to_code.clone())?;

    init(&mut args, shell, &config, &chain_config).await?;

    logger::success(MSG_CHAIN_INITIALIZED);
    Ok(())
}

pub async fn init(
    init_args: &mut InitArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    chain_config: &ChainConfig,
) -> anyhow::Result<()> {
    copy_configs(shell, &ecosystem_config.link_to_code, &chain_config.configs)?;

    let mut general_config = chain_config.get_general_config()?;
    apply_port_offset(init_args.port_offset, &mut general_config)?;
    let ports = ports_config(&general_config).context(MSG_PORTS_CONFIG_ERR)?;

    let consensus_keys = generate_consensus_keys();
    let consensus_config =
        get_consensus_config(chain_config, ports, Some(consensus_keys.clone()), None)?;
    general_config.consensus_config = Some(consensus_config);
    general_config.save_with_base_path(shell, &chain_config.configs)?;

    let mut genesis_config = chain_config.get_genesis_config()?;
    update_from_chain_config(&mut genesis_config, chain_config);
    genesis_config.save_with_base_path(shell, &chain_config.configs)?;

    // Copy ecosystem contracts
    let mut contracts_config = ecosystem_config.get_contracts_config()?;
    contracts_config.l1.base_token_addr = chain_config.base_token.address;
    contracts_config.save_with_base_path(shell, &chain_config.configs)?;

    distribute_eth(ecosystem_config, chain_config, init_args.l1_rpc_url.clone()).await?;
    mint_base_token(ecosystem_config, chain_config, init_args.l1_rpc_url.clone()).await?;

    let mut secrets = chain_config.get_secrets_config()?;
    set_l1_rpc_url(&mut secrets, init_args.l1_rpc_url.clone())?;
    secrets.consensus = Some(get_consensus_secrets(&consensus_keys));
    secrets.save_with_base_path(shell, &chain_config.configs)?;

    let spinner = Spinner::new(MSG_REGISTERING_CHAIN_SPINNER);
    register_chain(
        shell,
        init_args.forge_args.clone(),
        ecosystem_config,
        chain_config,
        &mut contracts_config,
        init_args.l1_rpc_url.clone(),
        None,
        true,
    )
    .await?;
    contracts_config.save_with_base_path(shell, &chain_config.configs)?;
    spinner.finish();
    let spinner = Spinner::new(MSG_ACCEPTING_ADMIN_SPINNER);
    accept_admin(
        shell,
        ecosystem_config,
        contracts_config.l1.chain_admin_addr,
        chain_config.get_wallets_config()?.governor_private_key(),
        contracts_config.l1.diamond_proxy_addr,
        &init_args.forge_args.clone(),
        init_args.l1_rpc_url.clone(),
    )
    .await?;
    spinner.finish();

    if chain_config.base_token != BaseToken::eth() {
        let spinner = Spinner::new(MSG_UPDATING_TOKEN_MULTIPLIER_SETTER_SPINNER);
        set_token_multiplier_setter(
            shell,
            ecosystem_config,
            chain_config.get_wallets_config()?.governor_private_key(),
            contracts_config.l1.chain_admin_addr,
            chain_config
                .get_wallets_config()
                .unwrap()
                .token_multiplier_setter
                .context(MSG_WALLET_TOKEN_MULTIPLIER_SETTER_NOT_FOUND)?
                .address,
            &init_args.forge_args.clone(),
            init_args.l1_rpc_url.clone(),
        )
        .await?;
        spinner.finish();
    }

    deploy_l2_contracts::deploy_l2_contracts(
        shell,
        chain_config,
        ecosystem_config,
        &mut contracts_config,
        init_args.forge_args.clone(),
    )
    .await?;
    contracts_config.save_with_base_path(shell, &chain_config.configs)?;

    if let Some(true) = chain_config.legacy_bridge {
        setup_legacy_bridge(
            shell,
            chain_config,
            ecosystem_config,
            &contracts_config,
            init_args.forge_args.clone(),
        )
        .await?;
    }

    if init_args.deploy_paymaster {
        let spinner = Spinner::new(MSG_DEPLOYING_PAYMASTER);
        deploy_paymaster::deploy_paymaster(
            shell,
            chain_config,
            &mut contracts_config,
            init_args.forge_args.clone(),
            None,
            true,
        )
        .await?;
        contracts_config.save_with_base_path(shell, &chain_config.configs)?;
        spinner.finish();
    }

    genesis(init_args.genesis_args.clone(), shell, chain_config)
        .await
        .context(MSG_GENESIS_DATABASE_ERR)?;

    update_portal_config(shell, chain_config)
        .await
        .context(MSG_PORTAL_FAILED_TO_CREATE_CONFIG_ERR)?;

    Ok(())
}

fn apply_port_offset(port_offset: u16, general_config: &mut GeneralConfig) -> anyhow::Result<()> {
    let Some(mut ports_config) = ports_config(general_config) else {
        bail!("Missing ports config");
    };

    ports_config.apply_offset(port_offset);

    update_ports(general_config, &ports_config)?;

    Ok(())
}
