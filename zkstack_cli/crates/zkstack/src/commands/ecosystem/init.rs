use std::{path::PathBuf, str::FromStr};

use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{
    contracts::rebuild_all_contracts,
    forge::{ForgeRunner, ForgeScriptArgs},
    logger,
    spinner::Spinner,
    Prompt,
};
use zkstack_cli_config::{
    forge_interface::deploy_ecosystem::input::InitialDeploymentConfig,
    traits::{FileConfigWithDefaultName, SaveConfigWithBasePath},
    ContractsConfig, CoreContractsConfig, EcosystemConfig, ZkStackConfig,
};
use zkstack_cli_types::{L1Network, VMOption};
use zksync_basic_types::Address;

use super::{
    args::init::{EcosystemInitArgs, EcosystemInitArgsFinal},
    common::init_chains,
    setup_observability,
};
use crate::{
    admin_functions::{accept_admin, accept_owner},
    commands::{
        ctm::commands::init_new_ctm::deploy_new_ctm_and_accept_admin,
        ecosystem::{
            common::{deploy_erc20, deploy_l1_core_contracts},
            create_configs::{create_erc20_deployment_config, create_initial_deployments_config},
            register_ctm::register_ctm_on_existing_bh,
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

    let initial_deployment_config = match ecosystem_config.get_initial_deployment_config() {
        Ok(config) => config,
        Err(_) => create_initial_deployments_config(shell, &ecosystem_config.config)?,
    };

    let final_ecosystem_args = args
        .fill_values_with_prompt(ecosystem_config.l1_network)
        .await?;
    let mut runner = ForgeRunner::new(final_ecosystem_args.forge_args.runner.clone());
    logger::info(MSG_INITIALIZING_ECOSYSTEM);

    if final_ecosystem_args.observability {
        setup_observability::run(shell)?;
    }

    let contracts_config = init_ecosystem(
        &final_ecosystem_args.clone(),
        shell,
        &mut runner,
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
            &mut runner,
            &erc20_deployment_config,
            &ecosystem_config,
            &contracts_config.into(),
            final_ecosystem_args.forge_args.script.clone(),
            final_ecosystem_args.l1_rpc_url.clone(),
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
    runner: &mut ForgeRunner,
    ecosystem_config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
) -> anyhow::Result<CoreContractsConfig> {
    let spinner = Spinner::new(MSG_INTALLING_DEPS_SPINNER);
    spinner.finish();

    let contracts = if !init_args.deploy_ecosystem {
        return_ecosystem_contracts(
            shell,
            init_args.ecosystem_contracts_path.clone(),
            ecosystem_config,
        )
        .await?
    } else {
        let core_contracts = deploy_ecosystem(
            shell,
            runner,
            init_args.l1_rpc_url.clone(),
            init_args.forge_args.script.clone(),
            ecosystem_config,
            initial_deployment_config,
            init_args.support_l2_legacy_shared_bridge_test,
            init_args.vm_option,
        )
        .await?;
        core_contracts.save_with_base_path(shell, &ecosystem_config.config)?;

        let mut contracts = deploy_and_register_ctm(
            shell,
            runner,
            init_args.l1_rpc_url.clone(),
            ecosystem_config,
            initial_deployment_config,
            core_contracts.core_ecosystem_contracts.bridgehub_proxy_addr,
            init_args.support_l2_legacy_shared_bridge_test,
            &init_args.forge_args.script,
            init_args.vm_option,
        )
        .await?;

        // If we are deploying non-zksync os ecosystem, but zksync os ecosystem config exists
        if !init_args.vm_option.is_zksync_os()
            && ecosystem_config.zksync_os_exist()
            && init_args.dev
            && !init_args.forge_args.runner.is_docker()
        {
            rebuild_all_contracts(
                shell,
                &ecosystem_config.contracts_path_for_ctm(VMOption::ZKSyncOsVM),
            )?;
            contracts = deploy_and_register_ctm(
                shell,
                runner,
                init_args.l1_rpc_url.clone(),
                ecosystem_config,
                initial_deployment_config,
                core_contracts.core_ecosystem_contracts.bridgehub_proxy_addr,
                init_args.support_l2_legacy_shared_bridge_test,
                &init_args.forge_args.script,
                VMOption::ZKSyncOsVM,
            )
            .await?;
        }

        contracts
    };
    Ok(contracts)
}

#[allow(clippy::too_many_arguments)]
async fn deploy_and_register_ctm(
    shell: &Shell,
    runner: &mut ForgeRunner,
    l1_rpc_url: String,
    ecosystem_config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
    bridgehub_proxy_addr: Address,
    support_l2_legacy_shared_bridge_test: bool,
    forge_args: &ForgeScriptArgs,
    vm_option: VMOption,
) -> anyhow::Result<CoreContractsConfig> {
    let contracts = deploy_new_ctm_and_accept_admin(
        shell,
        runner,
        l1_rpc_url.clone(),
        forge_args,
        ecosystem_config,
        initial_deployment_config,
        support_l2_legacy_shared_bridge_test,
        bridgehub_proxy_addr,
        vm_option,
        true,
    )
    .await?;

    contracts.save_with_base_path(shell, &ecosystem_config.config)?;

    register_ctm_on_existing_bh(
        shell,
        runner,
        forge_args,
        ecosystem_config,
        &l1_rpc_url,
        None,
        bridgehub_proxy_addr,
        contracts.ctm(vm_option).state_transition_proxy_addr,
        false,
        vm_option,
    )
    .await?;
    Ok(contracts)
}

async fn return_ecosystem_contracts(
    shell: &Shell,
    ecosystem_contracts_path: Option<PathBuf>,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<CoreContractsConfig> {
    let ecosystem_contracts_path = match &ecosystem_contracts_path {
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

    // We don't have a zksync os preexisting contracts config, so we can assume
    // that it's always false during fallback
    CoreContractsConfig::read_with_fallback(shell, ecosystem_contracts_path, VMOption::EraVM)
}

async fn deploy_ecosystem(
    shell: &Shell,
    runner: &mut ForgeRunner,
    l1_rpc_url: String,
    forge_args: ForgeScriptArgs,
    ecosystem_config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
    support_l2_legacy_shared_bridge_test: bool,
    vm_option: VMOption,
) -> anyhow::Result<CoreContractsConfig> {
    let spinner = Spinner::new(MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER);
    let contracts_config = deploy_l1_core_contracts(
        shell,
        runner,
        &forge_args,
        ecosystem_config,
        initial_deployment_config,
        &l1_rpc_url,
        None,
        true,
        support_l2_legacy_shared_bridge_test,
        vm_option,
    )
    .await?;
    spinner.finish();

    accept_owner(
        shell,
        runner,
        ecosystem_config.path_to_foundry_scripts_for_ctm(vm_option),
        contracts_config.l1.governance_addr,
        &ecosystem_config.get_wallets()?.governor,
        contracts_config
            .core_ecosystem_contracts
            .bridgehub_proxy_addr,
        &forge_args,
        l1_rpc_url.clone(),
    )
    .await?;
    accept_admin(
        shell,
        runner,
        ecosystem_config.path_to_foundry_scripts_for_ctm(vm_option),
        contracts_config.l1.chain_admin_addr,
        &ecosystem_config.get_wallets()?.governor,
        contracts_config
            .core_ecosystem_contracts
            .bridgehub_proxy_addr,
        &forge_args,
        l1_rpc_url.clone(),
    )
    .await?;

    // Note, that there is no admin in L1 asset router, so we do
    // need to accept it
    accept_owner(
        shell,
        runner,
        ecosystem_config.path_to_foundry_scripts_for_ctm(vm_option),
        contracts_config.l1.governance_addr,
        &ecosystem_config.get_wallets()?.governor,
        contracts_config.bridges.shared.l1_address,
        &forge_args,
        l1_rpc_url.clone(),
    )
    .await?;

    accept_owner(
        shell,
        runner,
        ecosystem_config.path_to_foundry_scripts_for_ctm(vm_option),
        contracts_config.l1.governance_addr,
        &ecosystem_config.get_wallets()?.governor,
        contracts_config
            .core_ecosystem_contracts
            .stm_deployment_tracker_proxy_addr
            .context("stm_deployment_tracker_proxy_addr")?,
        &forge_args,
        l1_rpc_url.clone(),
    )
    .await?;

    Ok(contracts_config)
}
