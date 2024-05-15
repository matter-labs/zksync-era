use anyhow::Context;
use common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    logger,
    spinner::Spinner,
};
use xshell::Shell;

use super::args::init::InitArgsFinal;
use crate::{
    commands::hyperchain::{
        args::init::InitArgs, deploy_paymaster, genesis::genesis, initialize_bridges,
    },
    configs::{
        copy_configs,
        forge_interface::register_hyperchain::{
            input::RegisterHyperchainL1Config, output::RegisterHyperchainOutput,
        },
        update_diamond_proxy, update_genesis, ContractsConfig, EcosystemConfig, HyperchainConfig,
        ReadConfig, SaveConfig,
    },
    consts::{CONTRACTS_FILE, REGISTER_HYPERCHAIN},
    forge_utils::fill_forge_private_key,
};

pub(crate) async fn run(args: InitArgs, shell: &Shell) -> anyhow::Result<()> {
    let hyperchain_name = global_config().hyperchain_name.clone();
    let config = EcosystemConfig::from_file()?;
    let hyperchain_config = config
        .load_hyperchain(hyperchain_name)
        .context("Hyperchain not found")?;
    let mut args = args.fill_values_with_prompt(&hyperchain_config);

    logger::note(
        "Selected config:",
        &logger::object_to_string(&hyperchain_config),
    );
    logger::info("Initializing hyperchain");

    init(&mut args, shell, &config, &hyperchain_config).await?;

    logger::success("Hyperchain initialized successfully");
    Ok(())
}

pub async fn init(
    init_args: &mut InitArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    hyperchain_config: &HyperchainConfig,
) -> anyhow::Result<()> {
    copy_configs(
        shell,
        &ecosystem_config.link_to_code,
        &hyperchain_config.configs,
    )?;

    update_genesis(shell, hyperchain_config)?;
    let mut contracts_config = ContractsConfig::read(ecosystem_config.config.join(CONTRACTS_FILE))?;
    contracts_config.l1.base_token_addr = hyperchain_config.base_token.address;
    // Copy ecosystem contracts
    contracts_config.save(shell, hyperchain_config.configs.join(CONTRACTS_FILE))?;

    let spinner = Spinner::new("Registering hyperchain...");
    register_hyperchain(
        shell,
        init_args.forge_args.clone(),
        ecosystem_config,
        hyperchain_config,
    )
    .await?;
    spinner.finish();

    initialize_bridges::run(init_args.forge_args.clone(), shell)?;

    if init_args.deploy_paymaster {
        deploy_paymaster::run(init_args.forge_args.clone(), shell)?
    }

    genesis(
        init_args.genesis_args.clone(),
        shell,
        hyperchain_config,
        ecosystem_config,
    )
    .await
    .context("Unable to perform genesis on the database")?;

    Ok(())
}

async fn register_hyperchain(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    config: &EcosystemConfig,
    hyperchain_config: &HyperchainConfig,
) -> anyhow::Result<()> {
    let deploy_config_path = REGISTER_HYPERCHAIN.input(&config.link_to_code);

    let contracts = config
        .get_contracts_config()
        .context("Ecosystem contracts config not found")?;
    let deploy_config = RegisterHyperchainL1Config::new(hyperchain_config, &contracts)?;
    deploy_config.save(shell, deploy_config_path)?;

    let mut forge = Forge::new(&config.path_to_foundry())
        .script(&REGISTER_HYPERCHAIN.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(config.l1_rpc_url.clone())
        .with_broadcast();

    forge = fill_forge_private_key(forge, config.get_wallets()?.governor_private_key())?;

    forge.run(shell)?;

    let register_hyperchain_output = RegisterHyperchainOutput::read(
        REGISTER_HYPERCHAIN.output(&hyperchain_config.link_to_code),
    )?;
    update_diamond_proxy(shell, hyperchain_config, &register_hyperchain_output)?;
    Ok(())
}
