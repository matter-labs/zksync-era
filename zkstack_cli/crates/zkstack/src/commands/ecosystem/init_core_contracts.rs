use std::{path::PathBuf, str::FromStr};

use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{
    contracts::{build_l1_contracts, build_l2_contracts, build_system_contracts},
    forge::ForgeScriptArgs,
    git, logger,
    spinner::Spinner,
    Prompt,
};
use zkstack_cli_config::{
    forge_interface::deploy_ecosystem::input::InitialDeploymentConfig,
    traits::{FileConfigWithDefaultName, ReadConfig, SaveConfigWithBasePath},
    ContractsConfig, EcosystemConfig,
};
use zkstack_cli_types::L1Network;

use super::{
    args::init::{EcosystemArgsFinal, EcosystemInitArgs, EcosystemInitArgsFinal},
    setup_observability,
    utils::{build_da_contracts, install_yarn_dependencies},
};
use crate::{
    admin_functions::{accept_admin, accept_owner},
    commands::ecosystem::{
        args::init::PromptPolicy,
        common::{deploy_erc20, deploy_l1_core_contracts},
        create_configs::{create_erc20_deployment_config, create_initial_deployments_config},
    },
    messages::{
        msg_ecosystem_no_found_preexisting_contract, MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER,
        MSG_DEPLOYING_ERC20, MSG_ECOSYSTEM_CONTRACTS_PATH_INVALID_ERR,
        MSG_ECOSYSTEM_CONTRACTS_PATH_PROMPT, MSG_INITIALIZING_ECOSYSTEM,
        MSG_INTALLING_DEPS_SPINNER,
    },
};

pub async fn run(args: EcosystemInitArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    if args.update_submodules.is_none() || args.update_submodules == Some(true) {
        git::submodule_update(shell, ecosystem_config.link_to_code.clone())?;
    }

    let initial_deployment_config = match ecosystem_config.get_initial_deployment_config() {
        Ok(config) => config,
        Err(_) => create_initial_deployments_config(shell, &ecosystem_config.config)?,
    };

    let prompt_policy = PromptPolicy {
        deploy_erc20: false,
        observability: false,
        skip_ecosystem: true,
    };

    let mut final_ecosystem_args = args
        .clone()
        .fill_values_with_prompt(ecosystem_config.l1_network, prompt_policy)
        .await?;

    logger::info(MSG_INITIALIZING_ECOSYSTEM);

    if final_ecosystem_args.observability {
        setup_observability::run(shell)?;
    }

    let contracts_config = init_ecosystem(
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

    Ok(())
}

async fn init_ecosystem(
    init_args: &mut EcosystemInitArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
) -> anyhow::Result<ContractsConfig> {
    let spinner = Spinner::new(MSG_INTALLING_DEPS_SPINNER);
    if !init_args.skip_contract_compilation_override {
        install_yarn_dependencies(shell, &ecosystem_config.link_to_code)?;
        build_da_contracts(shell, &ecosystem_config.link_to_code)?;
        build_l1_contracts(shell.clone(), ecosystem_config.link_to_code.clone())?;
        build_system_contracts(shell.clone(), ecosystem_config.link_to_code.clone())?;
        build_l2_contracts(shell.clone(), ecosystem_config.link_to_code.clone())?;
    }
    spinner.finish();

    let contracts = deploy_ecosystem(
        shell,
        &mut init_args.ecosystem,
        init_args.forge_args.clone(),
        ecosystem_config,
        initial_deployment_config,
        init_args.support_l2_legacy_shared_bridge_test,
    )
    .await?;
    contracts.save_with_base_path(shell, &ecosystem_config.config)?;
    Ok(contracts)
}

async fn deploy_ecosystem(
    shell: &Shell,
    ecosystem: &mut EcosystemArgsFinal,
    forge_args: ForgeScriptArgs,
    ecosystem_config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
    support_l2_legacy_shared_bridge_test: bool,
) -> anyhow::Result<ContractsConfig> {
    if ecosystem.deploy_ecosystem {
        return deploy_ecosystem_inner(
            shell,
            forge_args,
            ecosystem_config,
            initial_deployment_config,
            ecosystem.l1_rpc_url.clone(),
            support_l2_legacy_shared_bridge_test,
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
    support_l2_legacy_shared_bridge_test: bool,
) -> anyhow::Result<ContractsConfig> {
    let spinner = Spinner::new(MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER);
    let contracts_config = deploy_l1_core_contracts(
        shell,
        &forge_args,
        config,
        initial_deployment_config,
        &l1_rpc_url,
        None,
        true,
        support_l2_legacy_shared_bridge_test,
    )
    .await?;
    spinner.finish();

    println!("Got here 1");
    accept_owner(
        shell,
        config,
        contracts_config.l1.governance_addr,
        &config.get_wallets()?.governor,
        contracts_config.ecosystem_contracts.bridgehub_proxy_addr,
        &forge_args,
        l1_rpc_url.clone(),
    )
    .await?;

    println!("Got here 2");

    accept_admin(
        shell,
        config,
        contracts_config.l1.chain_admin_addr,
        &config.get_wallets()?.governor,
        contracts_config.ecosystem_contracts.bridgehub_proxy_addr,
        &forge_args,
        l1_rpc_url.clone(),
    )
    .await?;

    println!("Got here 3");

    accept_owner(
        shell,
        config,
        contracts_config.l1.governance_addr,
        &config.get_wallets()?.governor,
        contracts_config.bridges.shared.l1_address,
        &forge_args,
        l1_rpc_url.clone(),
    )
    .await?;

    println!("Got here 4");

    accept_owner(
        shell,
        config,
        contracts_config.l1.governance_addr,
        &config.get_wallets()?.governor,
        contracts_config
            .ecosystem_contracts
            .stm_deployment_tracker_proxy_addr
            .context("stm_deployment_tracker_proxy_addr")?,
        &forge_args,
        l1_rpc_url.clone(),
    )
    .await?;

    println!("Got here 5");

    Ok(contracts_config)
}
