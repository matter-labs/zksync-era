use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::Context;
use common::{
    cmd::Cmd,
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    logger,
    spinner::Spinner,
    Prompt,
};
use xshell::{cmd, Shell};

use super::args::init::{EcosystemArgsFinal, EcosystemInitArgs, EcosystemInitArgsFinal};
use crate::{
    accept_ownership::accept_ownership,
    commands::hyperchain,
    configs::{
        forge_interface::deploy_ecosystem::{
            input::{
                DeployErc20Config, DeployL1Config, Erc20DeploymentConfig, InitialDeploymentConfig,
            },
            output::{DeployErc20Output, DeployL1Output},
        },
        ContractsConfig, EcosystemConfig, GenesisConfig, ReadConfig, SaveConfig,
    },
    consts::{
        CONFIGS_PATH, CONTRACTS_FILE, DEPLOY_ECOSYSTEM, DEPLOY_ERC20, ECOSYSTEM_PATH,
        ERC20_CONFIGS_FILE, GENESIS_FILE,
    },
    forge_utils::fill_forge_private_key,
    types::{L1Network, ProverMode},
};

pub async fn run(args: EcosystemInitArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file()?;

    let hyperchain_name = global_config().hyperchain_name.clone();
    let hyperchain_config = ecosystem_config
        .load_hyperchain(hyperchain_name)
        .context("Hyperchain not initialized. Please create a hyperchain first")?;

    let initial_deployment_config = ecosystem_config
        .get_initial_deployment_config()
        .context("Initial deployment config not found")?;

    let erc20_deployment_config = ecosystem_config
        .get_erc20_deployment_config()
        .context("ERC20 deployment config not found")?;

    let mut args = args.fill_values_with_prompt(&hyperchain_config);

    logger::info("Initializing ecosystem");

    let contracts_config = init(
        &mut args,
        shell,
        &ecosystem_config,
        &initial_deployment_config,
    )?;

    let mut hyperchain_init_args = hyperchain::args::init::InitArgsFinal {
        forge_args: args.forge_args.clone(),
        genesis_args: args.genesis_args.clone(),
        deploy_paymaster: args.deploy_paymaster,
    };

    logger::info("Initializing hyperchain");
    hyperchain::init::init(
        &mut hyperchain_init_args,
        shell,
        &ecosystem_config,
        &hyperchain_config,
    )
    .await?;

    if args.deploy_erc20 {
        logger::info("Deploying ERC20 contracts");
        deploy_erc20(
            shell,
            &erc20_deployment_config,
            &ecosystem_config,
            &contracts_config,
            args.forge_args.clone(),
        )?;
    }

    logger::outro("Ecosystem initialized successfully");

    Ok(())
}

fn init(
    init_args: &mut EcosystemInitArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
) -> anyhow::Result<ContractsConfig> {
    let spinner = Spinner::new("Installing and building dependencies...");
    install_yarn_dependencies(shell, &ecosystem_config.link_to_code)?;
    build_system_contracts(shell, &ecosystem_config.link_to_code)?;
    spinner.finish();

    let contracts = deploy_ecosystem(
        shell,
        &mut init_args.ecosystem,
        init_args.forge_args.clone(),
        ecosystem_config,
        initial_deployment_config,
    )?;
    contracts.save(shell, ecosystem_config.config.clone().join(CONTRACTS_FILE))?;
    Ok(contracts)
}

fn deploy_erc20(
    shell: &Shell,
    erc20_deployment_config: &Erc20DeploymentConfig,
    ecosystem_config: &EcosystemConfig,
    contracts_config: &ContractsConfig,
    forge_args: ForgeScriptArgs,
) -> anyhow::Result<DeployErc20Output> {
    let deploy_config_path = DEPLOY_ERC20.input(&ecosystem_config.link_to_code);
    DeployErc20Config::new(erc20_deployment_config, contracts_config)
        .save(shell, deploy_config_path)?;

    let mut forge = Forge::new(&ecosystem_config.path_to_foundry())
        .script(&DEPLOY_ERC20.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(ecosystem_config.l1_rpc_url.clone())
        .with_broadcast();

    forge = fill_forge_private_key(
        forge,
        ecosystem_config.get_wallets()?.deployer_private_key(),
    )?;

    let spinner = Spinner::new("Deploying ERC20 contracts...");
    forge.run(shell)?;
    spinner.finish();

    let result = DeployErc20Output::read(DEPLOY_ERC20.output(&ecosystem_config.link_to_code))?;
    result.save(shell, ecosystem_config.config.join(ERC20_CONFIGS_FILE))?;
    Ok(result)
}

fn deploy_ecosystem(
    shell: &Shell,
    ecosystem: &mut EcosystemArgsFinal,
    forge_args: ForgeScriptArgs,
    ecosystem_config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
) -> anyhow::Result<ContractsConfig> {
    if ecosystem.deploy_ecosystem {
        return deploy_ecosystem_inner(
            shell,
            forge_args,
            ecosystem_config,
            initial_deployment_config,
        );
    }

    let ecosystem_contracts_path = match &ecosystem.ecosystem_contracts_path {
        Some(path) => Some(path.clone()),
        None => {
            let input_path: String = Prompt::new("Provide the path to the ecosystem contracts or keep it empty and you will be added to ZkSync ecosystem")
            .allow_empty()
            .validate_with(|val: &String| {
                if val.is_empty() {
                    return Ok(());
                }
                PathBuf::from_str(val).map(|_| ()).map_err(|_| "Invalid path".to_string())
            })
            .ask();
            if input_path.is_empty() {
                None
            } else {
                Some(input_path.into())
            }
        }
    };

    let ecosystem_contracts_path =
        ecosystem_contracts_path.unwrap_or_else(|| match ecosystem_config.l1_network {
            L1Network::Localhost => ecosystem_config.config.join(CONTRACTS_FILE),
            L1Network::Sepolia => ecosystem_config
                .link_to_code
                .join(ECOSYSTEM_PATH)
                .join(ecosystem_config.l1_network.to_string().to_lowercase()),
            L1Network::Mainnet => ecosystem_config
                .link_to_code
                .join(ECOSYSTEM_PATH)
                .join(ecosystem_config.l1_network.to_string().to_lowercase()),
        });

    ContractsConfig::read(ecosystem_contracts_path)
}

fn deploy_ecosystem_inner(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
) -> anyhow::Result<ContractsConfig> {
    let deploy_config_path = DEPLOY_ECOSYSTEM.input(&config.link_to_code);

    let default_genesis_config =
        GenesisConfig::read(config.link_to_code.join(CONFIGS_PATH).join(GENESIS_FILE))
            .context("Context")?;

    let wallets_config = config.get_wallets()?;
    // For deploying ecosystem we only need genesis batch params
    let deploy_config = DeployL1Config::new(
        &default_genesis_config,
        &wallets_config,
        initial_deployment_config,
        config.era_chain_id,
        config.prover_version == ProverMode::NoProofs,
    );
    deploy_config.save(shell, deploy_config_path)?;

    let mut forge = Forge::new(&config.path_to_foundry())
        .script(&DEPLOY_ECOSYSTEM.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(config.l1_rpc_url.clone())
        .with_broadcast()
        .with_slow();

    forge = fill_forge_private_key(forge, wallets_config.deployer_private_key())?;

    let spinner = Spinner::new("Deploying ecosystem contracts...");
    forge.run(shell)?;
    spinner.finish();

    let script_output = DeployL1Output::read(DEPLOY_ECOSYSTEM.output(&config.link_to_code))?;
    let mut contracts_config = ContractsConfig::default();
    contracts_config.update_from_l1_output(&script_output);
    accept_ownership(
        shell,
        config,
        contracts_config.l1.governance_addr,
        config.get_wallets()?.governor_private_key(),
        contracts_config.ecosystem_contracts.bridgehub_proxy_addr,
        &forge_args,
    )?;

    accept_ownership(
        shell,
        config,
        contracts_config.l1.governance_addr,
        config.get_wallets()?.governor_private_key(),
        contracts_config.bridges.shared.l1_address,
        &forge_args,
    )?;
    Ok(contracts_config)
}

fn install_yarn_dependencies(shell: &Shell, link_to_code: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code);
    Cmd::new(cmd!(shell, "yarn install")).run()
}

fn build_system_contracts(shell: &Shell, link_to_code: &Path) -> anyhow::Result<()> {
    let _dir_guard = shell.push_dir(link_to_code.join("contracts"));
    Cmd::new(cmd!(shell, "yarn sc build")).run()
}
