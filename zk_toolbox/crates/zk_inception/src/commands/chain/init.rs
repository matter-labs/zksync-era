use anyhow::Context;
use common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    logger,
    spinner::Spinner,
};
use xshell::Shell;

use super::args::init::InitArgsFinal;
use crate::forge_utils::check_the_balance;
use crate::{
    accept_ownership::accept_admin,
    commands::chain::{
        args::init::InitArgs, deploy_paymaster, genesis::genesis, initialize_bridges,
    },
    configs::{
        copy_configs,
        forge_interface::register_chain::{
            input::RegisterChainL1Config, output::RegisterChainOutput,
        },
        update_genesis, update_l1_contracts, ChainConfig, ContractsConfig, EcosystemConfig,
        ReadConfig, SaveConfig,
    },
    consts::{CONTRACTS_FILE, REGISTER_CHAIN},
    forge_utils::fill_forge_private_key,
};

pub(crate) async fn run(
    args: InitArgs,
    shell: &Shell,
    ecosystem_config: EcosystemConfig,
) -> anyhow::Result<()> {
    let chain_name = global_config().chain_name.clone();
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context("Chain not found")?;
    let mut args = args.fill_values_with_prompt(&chain_config);

    logger::note("Selected config:", logger::object_to_string(&chain_config));
    logger::info("Initializing chain");

    init(&mut args, shell, &ecosystem_config, &chain_config).await?;

    logger::success("Chain initialized successfully");
    Ok(())
}

pub(crate) fn load_global_config(
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<ChainConfig> {
    let chain_name = global_config().chain_name.clone();
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context("Chain not initialized. Please create a chain first")?;

    Ok(chain_config)
}

pub async fn init(
    init_args: &mut InitArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    chain_config: &ChainConfig,
) -> anyhow::Result<()> {
    copy_configs(shell, &ecosystem_config.link_to_code, &chain_config.configs)?;

    update_genesis(shell, chain_config)?;
    let mut contracts_config =
        ContractsConfig::read(shell, ecosystem_config.config.join(CONTRACTS_FILE))?;
    contracts_config.l1.base_token_addr = chain_config.base_token.address;
    // Copy ecosystem contracts
    contracts_config.save(shell, chain_config.configs.join(CONTRACTS_FILE))?;

    let spinner = Spinner::new("Registering chain...");
    contracts_config = register_chain(
        shell,
        init_args.forge_args.clone(),
        ecosystem_config,
        chain_config,
    )
    .await?;
    spinner.finish();
    let spinner = Spinner::new("Accepting admin...");
    accept_admin(
        shell,
        ecosystem_config,
        contracts_config.l1.governance_addr,
        chain_config.get_wallets_config()?.governor_private_key(),
        contracts_config.l1.diamond_proxy_addr,
        &init_args.forge_args.clone(),
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
        deploy_paymaster::deploy_paymaster(
            shell,
            chain_config,
            ecosystem_config,
            init_args.forge_args.clone(),
        )
        .await?;
    }

    genesis(
        init_args.genesis_args.clone(),
        shell,
        chain_config,
        ecosystem_config,
    )
    .await
    .context("Unable to perform genesis on the database")?;

    Ok(())
}

async fn register_chain(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    config: &EcosystemConfig,
    chain_config: &ChainConfig,
) -> anyhow::Result<ContractsConfig> {
    let deploy_config_path = REGISTER_CHAIN.input(&config.link_to_code);

    let contracts = config
        .get_contracts_config()
        .context("Ecosystem contracts config not found")?;
    let deploy_config = RegisterChainL1Config::new(chain_config, &contracts)?;
    deploy_config.save(shell, deploy_config_path)?;

    let mut forge = Forge::new(&config.path_to_foundry())
        .script(&REGISTER_CHAIN.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(config.l1_rpc_url.clone())
        .with_broadcast();

    forge = fill_forge_private_key(forge, config.get_wallets()?.governor_private_key())?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;

    let register_chain_output =
        RegisterChainOutput::read(shell, REGISTER_CHAIN.output(&chain_config.link_to_code))?;
    update_l1_contracts(shell, chain_config, &register_chain_output)
}
