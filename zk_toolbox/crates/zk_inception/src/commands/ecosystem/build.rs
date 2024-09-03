use std::{path::PathBuf, str::FromStr};

use anyhow::Context;
use common::{
    forge::{Forge, ForgeScriptArgs},
    git, logger,
    spinner::Spinner,
    Prompt,
};
use config::{
    forge_interface::{
        deploy_ecosystem::{
            input::{
                DeployErc20Config, DeployL1Config, Erc20DeploymentConfig, InitialDeploymentConfig,
            },
            output::{DeployL1Output, ERC20Tokens},
        },
        script_params::{DEPLOY_ECOSYSTEM_SCRIPT_PARAMS, DEPLOY_ERC20_SCRIPT_PARAMS},
    },
    traits::{
        FileConfigWithDefaultName, ReadConfig, ReadConfigWithBasePath, SaveConfig,
        SaveConfigWithBasePath,
    },
    ContractsConfig, EcosystemConfig, GenesisConfig,
};
use types::{L1Network, ProverMode};
use xshell::Shell;

use super::{
    args::build::{EcosystemArgsFinal, EcosystemBuildArgs, EcosystemBuildArgsFinal},
    utils::{build_system_contracts, install_yarn_dependencies},
};
use crate::{
    accept_ownership::accept_owner,
    commands::ecosystem::create_configs::{
        create_erc20_deployment_config, create_initial_deployments_config,
    },
    messages::{
        msg_ecosystem_no_found_preexisting_contract, MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER,
        MSG_DEPLOYING_ERC20, MSG_DEPLOYING_ERC20_SPINNER, MSG_ECOSYSTEM_CONTRACTS_PATH_INVALID_ERR,
        MSG_ECOSYSTEM_CONTRACTS_PATH_PROMPT, MSG_INITIALIZING_ECOSYSTEM,
        MSG_INTALLING_DEPS_SPINNER,
    },
    utils::forge::{check_the_balance, fill_forge_private_key},
};

pub async fn run(args: EcosystemBuildArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    git::submodule_update(shell, ecosystem_config.link_to_code.clone())?;

    let initial_deployment_config = match ecosystem_config.get_initial_deployment_config() {
        Ok(config) => config,
        Err(_) => create_initial_deployments_config(shell, &ecosystem_config.config)?,
    };

    let mut final_ecosystem_args = args.fill_values_with_prompt(ecosystem_config.l1_network);

    logger::info(MSG_INITIALIZING_ECOSYSTEM);

    let contracts_config = init(
        &mut final_ecosystem_args,
        shell,
        &ecosystem_config,
        &initial_deployment_config,
    )
    .await?;

    if final_ecosystem_args.deploy_erc20 {
        logger::info(MSG_DEPLOYING_ERC20);
        let erc20_deployment_config = match ecosystem_config.get_erc20_deployment_config() {
            Ok(config) => config,
            Err(_) => create_erc20_deployment_config(shell, &ecosystem_config.config)?,
        };
        deploy_erc20(
            shell,
            &erc20_deployment_config,
            &ecosystem_config,
            &contracts_config,
            final_ecosystem_args.forge_args.clone(),
            final_ecosystem_args.ecosystem.l1_rpc_url.clone(),
        )
        .await?;
    }

    logger::outro("TODO");

    Ok(())
}

async fn init(
    init_args: &mut EcosystemBuildArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
) -> anyhow::Result<ContractsConfig> {
    let spinner = Spinner::new(MSG_INTALLING_DEPS_SPINNER);
    install_yarn_dependencies(shell, &ecosystem_config.link_to_code)?;
    build_system_contracts(shell, &ecosystem_config.link_to_code)?;
    spinner.finish();

    let contracts = deploy_ecosystem(
        shell,
        &mut init_args.ecosystem,
        init_args.forge_args.clone(),
        ecosystem_config,
        initial_deployment_config,
    )
    .await?;
    contracts.save_with_base_path(shell, &ecosystem_config.config)?;
    Ok(contracts)
}

async fn deploy_erc20(
    shell: &Shell,
    erc20_deployment_config: &Erc20DeploymentConfig,
    ecosystem_config: &EcosystemConfig,
    contracts_config: &ContractsConfig,
    forge_args: ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<ERC20Tokens> {
    let deploy_config_path = DEPLOY_ERC20_SCRIPT_PARAMS.input(&ecosystem_config.link_to_code);
    let wallets = ecosystem_config.get_wallets()?;
    DeployErc20Config::new(
        erc20_deployment_config,
        contracts_config,
        vec![
            wallets.governor.address,
            wallets.operator.address,
            wallets.blob_operator.address,
        ],
    )
    .save(shell, deploy_config_path)?;

    let mut forge = Forge::new(&ecosystem_config.path_to_foundry())
        .script(&DEPLOY_ERC20_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast();

    forge = fill_forge_private_key(
        forge,
        ecosystem_config.get_wallets()?.deployer_private_key(),
    )?;

    let spinner = Spinner::new(MSG_DEPLOYING_ERC20_SPINNER);
    check_the_balance(&forge).await?;
    forge.run(shell)?;
    spinner.finish();

    let result = ERC20Tokens::read(
        shell,
        DEPLOY_ERC20_SCRIPT_PARAMS.output(&ecosystem_config.link_to_code),
    )?;
    result.save_with_base_path(shell, &ecosystem_config.config)?;
    Ok(result)
}

async fn deploy_ecosystem(
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
            ecosystem.l1_rpc_url.clone(),
        )
        .await;
    }

    let ecosystem_contracts_path = match &ecosystem.ecosystem_contracts_path {
        Some(path) => Some(path.clone()),
        None => {
            let input_path: String = Prompt::new(MSG_ECOSYSTEM_CONTRACTS_PATH_PROMPT)
                .allow_empty()
                .validate_with(|val: &String| {
                    if val.is_empty() {
                        return Ok(());
                    }
                    PathBuf::from_str(val)
                        .map(|_| ())
                        .map_err(|_| MSG_ECOSYSTEM_CONTRACTS_PATH_INVALID_ERR.to_string())
                })
                .ask();
            if input_path.is_empty() {
                None
            } else {
                Some(input_path.into())
            }
        }
    };

    let ecosystem_preexisting_configs_path =
        ecosystem_config
            .get_preexisting_configs_path()
            .join(format!(
                "{}.yaml",
                ecosystem_config.l1_network.to_string().to_lowercase()
            ));

    // currently there are not some preexisting ecosystem contracts in
    // chains, so we need check if this file exists.
    if ecosystem_contracts_path.is_none() && !ecosystem_preexisting_configs_path.exists() {
        anyhow::bail!(msg_ecosystem_no_found_preexisting_contract(
            &ecosystem_config.l1_network.to_string()
        ))
    }

    let ecosystem_contracts_path =
        ecosystem_contracts_path.unwrap_or_else(|| match ecosystem_config.l1_network {
            L1Network::Localhost => {
                ContractsConfig::get_path_with_base_path(&ecosystem_config.config)
            }
            L1Network::Sepolia | L1Network::Holesky | L1Network::Mainnet => {
                ecosystem_preexisting_configs_path
            }
        });

    ContractsConfig::read(shell, ecosystem_contracts_path)
}

async fn deploy_ecosystem_inner(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
    l1_rpc_url: String,
) -> anyhow::Result<ContractsConfig> {
    let deploy_config_path = DEPLOY_ECOSYSTEM_SCRIPT_PARAMS.input(&config.link_to_code);

    let default_genesis_config =
        GenesisConfig::read_with_base_path(shell, config.get_default_configs_path())
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
        .script(&DEPLOY_ECOSYSTEM_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url.clone())
        .with_broadcast();

    if config.l1_network == L1Network::Localhost {
        // It's a kludge for reth, just because it doesn't behave properly with large amount of txs
        forge = forge.with_slow();
    }

    // TODO add sender instead of private key
    // forge = fill_forge_private_key(forge, wallets_config.deployer_private_key())?;

    let spinner = Spinner::new(MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER);
    check_the_balance(&forge).await?;
    forge.run(shell)?;
    spinner.finish();

    let script_output = DeployL1Output::read(
        shell,
        DEPLOY_ECOSYSTEM_SCRIPT_PARAMS.output(&config.link_to_code),
    )?;
    let mut contracts_config = ContractsConfig::default();
    contracts_config.update_from_l1_output(&script_output);
    accept_owner(
        shell,
        config,
        contracts_config.l1.governance_addr,
        config.get_wallets()?.governor_private_key(),
        contracts_config.ecosystem_contracts.bridgehub_proxy_addr,
        &forge_args,
        l1_rpc_url.clone(),
    )
    .await?;

    accept_owner(
        shell,
        config,
        contracts_config.l1.governance_addr,
        config.get_wallets()?.governor_private_key(),
        contracts_config.bridges.shared.l1_address,
        &forge_args,
        l1_rpc_url.clone(),
    )
    .await?;

    accept_owner(
        shell,
        config,
        contracts_config.l1.governance_addr,
        config.get_wallets()?.governor_private_key(),
        contracts_config
            .ecosystem_contracts
            .state_transition_proxy_addr,
        &forge_args,
        l1_rpc_url.clone(),
    )
    .await?;

    Ok(contracts_config)
}
