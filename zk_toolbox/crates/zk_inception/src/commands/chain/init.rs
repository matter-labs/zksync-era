use anyhow::Context;
use common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    logger,
    spinner::Spinner,
};
use config::{
    copy_configs,
    forge_interface::{
        register_chain::{input::RegisterChainL1Config, output::RegisterChainOutput},
        script_params::REGISTER_CHAIN_SCRIPT_PARAMS,
    },
    traits::{ReadConfig, ReadConfigWithBasePath, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, ContractsConfig, EcosystemConfig,
};
use xshell::Shell;

use super::args::init::InitArgsFinal;
use crate::{
    accept_ownership::accept_admin,
    commands::chain::{
        args::init::InitArgs, deploy_paymaster, genesis::genesis, initialize_bridges,
    },
    config_manipulations::{update_genesis, update_l1_contracts, update_l1_rpc_url_secret},
    forge_utils::{check_the_balance, fill_forge_private_key},
    messages::{
        msg_initializing_chain, MSG_ACCEPTING_ADMIN_SPINNER, MSG_CHAIN_INITIALIZED,
        MSG_CHAIN_NOT_FOUND_ERR, MSG_CONTRACTS_CONFIG_NOT_FOUND_ERR, MSG_GENESIS_DATABASE_ERR,
        MSG_REGISTERING_CHAIN_SPINNER, MSG_SELECTED_CONFIG,
    },
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

    update_genesis(shell, chain_config)?;
    update_l1_rpc_url_secret(shell, chain_config, init_args.l1_rpc_url.clone())?;
    let mut contracts_config =
        ContractsConfig::read_with_base_path(shell, &ecosystem_config.config)?;
    contracts_config.l1.base_token_addr = chain_config.base_token.address;
    // Copy ecosystem contracts
    contracts_config.save_with_base_path(shell, &chain_config.configs)?;

    crate::commands::ecosystem::init::distribute_eth(
        &ecosystem_config,
        &chain_config,
        init_args.l1_rpc_url.clone(),
    )
    .await?;
    let spinner = Spinner::new(MSG_REGISTERING_CHAIN_SPINNER);
    contracts_config = register_chain(
        shell,
        init_args.forge_args.clone(),
        ecosystem_config,
        chain_config,
        init_args.l1_rpc_url.clone(),
    )
    .await?;
    spinner.finish();
    let spinner = Spinner::new(MSG_ACCEPTING_ADMIN_SPINNER);
    accept_admin(
        shell,
        ecosystem_config,
        contracts_config.l1.governance_addr,
        chain_config.get_wallets_config()?.governor_private_key(),
        contracts_config.l1.diamond_proxy_addr,
        &init_args.forge_args.clone(),
        init_args.l1_rpc_url.clone(),
    )
    .await?;
    spinner.finish();

    initialize_bridges::initialize_bridges(
        shell,
        chain_config,
        ecosystem_config,
        init_args.forge_args.clone(),
    )
    .await?;

    if init_args.deploy_paymaster {
        deploy_paymaster::deploy_paymaster(shell, chain_config, init_args.forge_args.clone())
            .await?;
    }

    genesis(init_args.genesis_args.clone(), shell, chain_config)
        .await
        .context(MSG_GENESIS_DATABASE_ERR)?;

    Ok(())
}

async fn register_chain(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    config: &EcosystemConfig,
    chain_config: &ChainConfig,
    l1_rpc_url: String,
) -> anyhow::Result<ContractsConfig> {
    let deploy_config_path = REGISTER_CHAIN_SCRIPT_PARAMS.input(&config.link_to_code);

    let contracts = config
        .get_contracts_config()
        .context(MSG_CONTRACTS_CONFIG_NOT_FOUND_ERR)?;
    let deploy_config = RegisterChainL1Config::new(chain_config, &contracts)?;
    deploy_config.save(shell, deploy_config_path)?;

    let mut forge = Forge::new(&config.path_to_foundry())
        .script(&REGISTER_CHAIN_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast();

    forge = fill_forge_private_key(forge, config.get_wallets()?.governor_private_key())?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    let register_chain_output = RegisterChainOutput::read(
        shell,
        REGISTER_CHAIN_SCRIPT_PARAMS.output(&chain_config.link_to_code),
    )?;
    update_l1_contracts(shell, chain_config, &register_chain_output)
}
