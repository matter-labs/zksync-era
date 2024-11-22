use anyhow::Context;
use clap::{command, Parser, Subcommand};
use common::{git, logger, spinner::Spinner};
use config::{
    get_default_era_chain_id,
    traits::{ReadConfig, SaveConfigWithBasePath},
    ChainConfig, EcosystemConfig, WalletsConfig,
};
use types::BaseToken;
use xshell::Shell;

use crate::{
    accept_ownership::accept_admin,
    commands::chain::{
        args::init::{
            configs::{InitConfigsArgs, InitConfigsArgsFinal},
            InitArgs, InitArgsFinal,
        },
        common::{distribute_eth, mint_base_token},
        deploy_l2_contracts, deploy_paymaster,
        genesis::genesis,
        init::configs::init_configs,
        register_chain::register_chain,
        set_token_multiplier_setter::set_token_multiplier_setter,
        setup_legacy_bridge::setup_legacy_bridge,
    },
    messages::{
        msg_initializing_chain, MSG_ACCEPTING_ADMIN_SPINNER, MSG_CHAIN_INITIALIZED,
        MSG_DEPLOYING_PAYMASTER, MSG_GENESIS_DATABASE_ERR, MSG_REGISTERING_CHAIN_SPINNER,
        MSG_SELECTED_CONFIG, MSG_UPDATING_TOKEN_MULTIPLIER_SETTER_SPINNER,
        MSG_WALLETS_CONFIG_MUST_BE_PRESENT, MSG_WALLET_TOKEN_MULTIPLIER_SETTER_NOT_FOUND,
    },
};

// Init subcommands
pub mod configs;

#[derive(Subcommand, Debug, Clone)]
pub enum ChainInitSubcommands {
    /// Initialize chain configs
    Configs(InitConfigsArgs),
}

#[derive(Parser, Debug)]
#[command()]
pub struct ChainInitCommand {
    #[command(subcommand)]
    command: Option<ChainInitSubcommands>,
    #[clap(flatten)]
    args: InitArgs,
}

pub(crate) async fn run(
    args: ChainInitCommand,
    shell: &Shell,
    chain: ChainConfig,
    ecosystem: Option<EcosystemConfig>,
) -> anyhow::Result<()> {
    match args.command {
        Some(ChainInitSubcommands::Configs(args)) => {
            configs::run(args, shell, chain, ecosystem).await
        }
        None => run_init(args.args, shell, chain, ecosystem).await,
    }
}

async fn run_init(
    args: InitArgs,
    shell: &Shell,
    chain: ChainConfig,
    ecosystem: Option<EcosystemConfig>,
) -> anyhow::Result<()> {
    let args = args.fill_values_with_prompt(ecosystem.clone(), &chain)?;

    logger::note(MSG_SELECTED_CONFIG, logger::object_to_string(&chain));
    logger::info(msg_initializing_chain(""));
    git::submodule_update(shell, chain.link_to_code.clone())?;

    init(&args, shell, ecosystem, &chain).await?;

    logger::success(MSG_CHAIN_INITIALIZED);
    Ok(())
}

pub async fn init(
    init_args: &InitArgsFinal,
    shell: &Shell,
    ecosystem: Option<EcosystemConfig>,
    chain_config: &ChainConfig,
) -> anyhow::Result<()> {
    // Initialize configs
    let mut init_configs_args = InitConfigsArgsFinal::from_chain_init_args(init_args);
    if ecosystem.is_none() {
        init_configs_args.no_port_reallocation = true;
    }
    let mut contracts_config = init_configs(&init_configs_args, shell, chain_config).await?;
    let wallets = WalletsConfig::read(shell, chain_config.l1_wallets_path.clone())?;

    // Fund some wallet addresses with ETH or base token (only for Localhost)
    distribute_eth(chain_config, init_args.l1_rpc_url.clone(), &wallets).await?;
    mint_base_token(chain_config, init_args.l1_rpc_url.clone(), &wallets).await?;

    // Register chain on BridgeHub (run by L1 Governor)
    let spinner = Spinner::new(MSG_REGISTERING_CHAIN_SPINNER);
    register_chain(
        shell,
        init_args.forge_args.clone(),
        chain_config,
        &mut contracts_config,
        &wallets,
        init_args.l1_rpc_url.clone(),
        None,
        true,
    )
    .await?;
    contracts_config.save_with_base_path(shell, &chain_config.configs)?;
    spinner.finish();

    // Accept ownership for DiamondProxy (run by L2 Governor)
    let spinner = Spinner::new(MSG_ACCEPTING_ADMIN_SPINNER);
    accept_admin(
        shell,
        &chain_config.path_to_foundry(),
        contracts_config.l1.chain_admin_addr,
        &chain_config.get_wallets_config()?.governor,
        contracts_config.l1.diamond_proxy_addr,
        &init_args.forge_args.clone(),
        init_args.l1_rpc_url.clone(),
    )
    .await?;
    spinner.finish();

    // Set token multiplier setter address (run by L2 Governor)
    if chain_config.base_token != BaseToken::eth() {
        let spinner = Spinner::new(MSG_UPDATING_TOKEN_MULTIPLIER_SETTER_SPINNER);
        set_token_multiplier_setter(
            shell,
            &chain_config.path_to_foundry(),
            &chain_config.get_wallets_config()?.governor,
            contracts_config.l1.chain_admin_addr,
            chain_config
                .get_wallets_config()
                .context(MSG_WALLETS_CONFIG_MUST_BE_PRESENT)?
                .token_multiplier_setter
                .context(MSG_WALLET_TOKEN_MULTIPLIER_SETTER_NOT_FOUND)?
                .address,
            &init_args.forge_args.clone(),
            init_args.l1_rpc_url.clone(),
        )
        .await?;
        spinner.finish();
    }

    // Deploy L2 contracts: L2SharedBridge, L2DefaultUpgrader, ... (run by L1 Governor)
    let era_chain_id = ecosystem
        .as_ref()
        .map(|ecosystem| ecosystem.era_chain_id)
        .unwrap_or(get_default_era_chain_id());
    deploy_l2_contracts::deploy_l2_contracts(
        shell,
        chain_config,
        era_chain_id,
        &wallets,
        &mut contracts_config,
        init_args.forge_args.clone(),
    )
    .await?;
    contracts_config.save_with_base_path(shell, &chain_config.configs)?;

    // Setup legacy bridge - shouldn't be used for new chains (run by L1 Governor)
    if let Some(true) = chain_config.legacy_bridge {
        setup_legacy_bridge(
            shell,
            chain_config,
            &wallets,
            &contracts_config,
            init_args.forge_args.clone(),
        )
        .await?;
    }

    // Deploy Paymaster contract (run by L2 Governor)
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

    Ok(())
}
