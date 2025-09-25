use std::{path::PathBuf, str::FromStr};

use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{forge::ForgeScriptArgs, logger, spinner::Spinner, Prompt};
use zkstack_cli_config::{
    forge_interface::deploy_ecosystem::input::InitialDeploymentConfig,
    traits::{FileConfigWithDefaultName, ReadConfig, SaveConfigWithBasePath},
    ContractsConfig, CoreContractsConfig, EcosystemConfig, ZkStackConfig, ZkStackConfigTrait,
};
use zkstack_cli_types::L1Network;

use super::{
    args::init::{EcosystemArgsFinal, EcosystemInitArgs, EcosystemInitArgsFinal},
    common::init_chains,
    setup_observability,
};
use crate::{
    admin_functions::{accept_admin, accept_owner},
    commands::{
        ctm::commands::{init_new_ctm::deploy_new_ctm, register_ctm::register_ctm_on_existing_bh},
        ecosystem::{
            common::{deploy_erc20, deploy_l1_core_contracts},
            create_configs::{create_erc20_deployment_config, create_initial_deployments_config},
        },
    },
    messages::{
        msg_ecosystem_initialized, msg_ecosystem_no_found_preexisting_contract,
        MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER, MSG_DEPLOYING_ERC20,
        MSG_ECOSYSTEM_CONTRACTS_PATH_INVALID_ERR, MSG_ECOSYSTEM_CONTRACTS_PATH_PROMPT,
        MSG_INITIALIZING_ECOSYSTEM, MSG_INTALLING_DEPS_SPINNER,
    },
};

pub async fn run(args: EcosystemInitArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;

    // if args.update_submodules.is_none() || args.update_submodules == Some(true) {
    //     git::submodule_update(shell, &ecosystem_config.link_to_code())?;
    // }

    let initial_deployment_config = match ecosystem_config.get_initial_deployment_config() {
        Ok(config) => config,
        Err(_) => create_initial_deployments_config(shell, &ecosystem_config.config)?,
    };

    let final_ecosystem_args = args
        .fill_values_with_prompt(ecosystem_config.l1_network)
        .await?;

    logger::info(MSG_INITIALIZING_ECOSYSTEM);

    if final_ecosystem_args.observability {
        setup_observability::run(shell)?;
    }

    let contracts_config = init_ecosystem(
        &final_ecosystem_args,
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
            &contracts_config.into(),
            final_ecosystem_args.forge_args.clone(),
            final_ecosystem_args.ecosystem.l1_rpc_url.clone(),
        )
        .await?;
    }

    // Initialize chain(s)
    let mut chains: Vec<String> = vec![];
    if !final_ecosystem_args.ecosystem_only {
        chains = init_chains(final_ecosystem_args.clone(), shell, &ecosystem_config).await?;
    }
    logger::outro(msg_ecosystem_initialized(&chains.join(",")));

    Ok(())
}

async fn init_ecosystem(
    init_args: &EcosystemInitArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
) -> anyhow::Result<CoreContractsConfig> {
    let spinner = Spinner::new(MSG_INTALLING_DEPS_SPINNER);
    // if !init_args.skip_contract_compilation_override {
    //     install_yarn_dependencies(shell, &ecosystem_config.link_to_code())?;
    //     build_da_contracts(shell, &ecosystem_config.contracts_path())?;
    //     build_l1_contracts(shell.clone(), &ecosystem_config.contracts_path())?;
    //     build_system_contracts(shell.clone(), &ecosystem_config.contracts_path())?;
    //     build_l2_contracts(shell.clone(), &ecosystem_config.contracts_path())?;
    // }
    spinner.finish();

    let contracts = if !init_args.deploy_ecosystem {
        todo!()
        // return_ecosystem_contracts(shell, &init_args.ecosystem, ecosystem_config).await?
    } else {
        let core_contracts = deploy_ecosystem(
            shell,
            &init_args.ecosystem,
            init_args.forge_args.clone(),
            ecosystem_config,
            initial_deployment_config,
            init_args.support_l2_legacy_shared_bridge_test,
        )
        .await?;
        core_contracts.save_with_base_path(shell, &ecosystem_config.config)?;

        let contracts = deploy_new_ctm(
            shell,
            &init_args.forge_args,
            ecosystem_config,
            initial_deployment_config,
            init_args.ecosystem.l1_rpc_url.as_str(),
            None,
            true,
            init_args.support_l2_legacy_shared_bridge_test,
            core_contracts.core_ecosystem_contracts.bridgehub_proxy_addr,
            init_args.zksync_os,
            true,
        )
        .await?;

        contracts.save_with_base_path(shell, &ecosystem_config.config)?;

        register_ctm_on_existing_bh(
            shell,
            &init_args.forge_args,
            ecosystem_config,
            &init_args.ecosystem.l1_rpc_url,
            None,
            contracts.core_ecosystem_contracts.bridgehub_proxy_addr,
            contracts
                .ctm(init_args.zksync_os)
                .state_transition_proxy_addr,
            false,
        )
        .await?;
        contracts
    };
    Ok(contracts)
}

async fn return_ecosystem_contracts(
    shell: &Shell,
    ecosystem: &EcosystemArgsFinal,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<ContractsConfig> {
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

async fn deploy_ecosystem(
    shell: &Shell,
    ecosystem: &EcosystemArgsFinal,
    forge_args: ForgeScriptArgs,
    ecosystem_config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
    support_l2_legacy_shared_bridge_test: bool,
) -> anyhow::Result<CoreContractsConfig> {
    let spinner = Spinner::new(MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER);
    let contracts_config = deploy_l1_core_contracts(
        shell,
        &forge_args,
        ecosystem_config,
        initial_deployment_config,
        &ecosystem.l1_rpc_url.clone(),
        None,
        true,
        support_l2_legacy_shared_bridge_test,
    )
    .await?;
    spinner.finish();

    accept_owner(
        shell,
        ecosystem_config.path_to_foundry_scripts(),
        contracts_config.l1.governance_addr,
        &ecosystem_config.get_wallets()?.governor,
        contracts_config
            .core_ecosystem_contracts
            .bridgehub_proxy_addr,
        &forge_args,
        ecosystem.l1_rpc_url.clone(),
    )
    .await?;
    accept_admin(
        shell,
        ecosystem_config.path_to_foundry_scripts(),
        contracts_config.l1.chain_admin_addr,
        &ecosystem_config.get_wallets()?.governor,
        contracts_config
            .core_ecosystem_contracts
            .bridgehub_proxy_addr,
        &forge_args,
        ecosystem.l1_rpc_url.clone(),
    )
    .await?;

    // Note, that there is no admin in L1 asset router, so we do
    // need to accept it
    accept_owner(
        shell,
        ecosystem_config.path_to_foundry_scripts(),
        contracts_config.l1.governance_addr,
        &ecosystem_config.get_wallets()?.governor,
        contracts_config.bridges.shared.l1_address,
        &forge_args,
        ecosystem.l1_rpc_url.clone(),
    )
    .await?;

    accept_owner(
        shell,
        ecosystem_config.path_to_foundry_scripts(),
        contracts_config.l1.governance_addr,
        &ecosystem_config.get_wallets()?.governor,
        contracts_config
            .core_ecosystem_contracts
            .stm_deployment_tracker_proxy_addr
            .context("stm_deployment_tracker_proxy_addr")?,
        &forge_args,
        ecosystem.l1_rpc_url.clone(),
    )
    .await?;

    Ok(contracts_config)
}
