use ethers::types::H160;
use xshell::Shell;
use zkstack_cli_common::{
    contracts::{build_l1_contracts, build_l2_contracts, build_system_contracts},
    forge::ForgeScriptArgs,
    git, logger,
    spinner::Spinner,
};
use zkstack_cli_config::{
    forge_interface::deploy_ecosystem::input::InitialDeploymentConfig,
    traits::SaveConfigWithBasePath, ContractsConfig, EcosystemConfig, ZkStackConfig,
    ZkStackConfigTrait,
};

use super::{
    args::init::{EcosystemArgsFinal, InitNewCTMArgs, InitNewCTMArgsFinal},
    common::deploy_l1,
    utils::{build_da_contracts, install_yarn_dependencies},
};
use crate::{
    admin_functions::{accept_admin, accept_owner},
    commands::ecosystem::create_configs::create_initial_deployments_config,
    messages::{
        MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER, MSG_INITIALIZING_CTM, MSG_INTALLING_DEPS_SPINNER,
    },
};

pub async fn run(args: InitNewCTMArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;

    if args.update_submodules.is_none() || args.update_submodules == Some(true) {
        git::submodule_update(shell, &ecosystem_config.link_to_code())?;
    }

    let initial_deployment_config = match ecosystem_config.get_initial_deployment_config() {
        Ok(config) => config,
        Err(_) => create_initial_deployments_config(shell, &ecosystem_config.config)?,
    };

    let mut init_ctm_args = args
        .clone()
        .fill_values_with_prompt(ecosystem_config.l1_network)
        .await?;

    logger::info(MSG_INITIALIZING_CTM);

    init_ctm(
        &mut init_ctm_args,
        shell,
        &ecosystem_config,
        &initial_deployment_config,
    )
    .await?;

    Ok(())
}

async fn init_ctm(
    init_args: &mut InitNewCTMArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
) -> anyhow::Result<ContractsConfig> {
    let spinner = Spinner::new(MSG_INTALLING_DEPS_SPINNER);
    if !init_args.skip_contract_compilation_override {
        install_yarn_dependencies(shell, &ecosystem_config.link_to_code())?;
        build_da_contracts(shell, &ecosystem_config.contracts_path())?;
        build_l1_contracts(shell.clone(), &ecosystem_config.contracts_path())?;
        build_system_contracts(shell.clone(), &ecosystem_config.contracts_path())?;
        build_l2_contracts(shell.clone(), &ecosystem_config.contracts_path())?;
    }
    spinner.finish();

    let contracts = deploy_new_ctm(
        shell,
        &mut init_args.ecosystem,
        init_args.forge_args.clone(),
        ecosystem_config,
        initial_deployment_config,
        init_args.support_l2_legacy_shared_bridge_test,
        init_args.bridgehub_address,
        init_args.zksync_os,
        init_args.reuse_gov_and_admin,
    )
    .await?;
    contracts.save_with_base_path(shell, &ecosystem_config.config)?;
    Ok(contracts)
}

#[allow(clippy::too_many_arguments)]
pub async fn deploy_new_ctm(
    shell: &Shell,
    ecosystem: &mut EcosystemArgsFinal,
    forge_args: ForgeScriptArgs,
    ecosystem_config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
    support_l2_legacy_shared_bridge_test: bool,
    bridgehub_address: H160,
    zksync_os: bool,
    reuse_gov_and_admin: bool,
) -> anyhow::Result<ContractsConfig> {
    let l1_rpc_url = ecosystem.l1_rpc_url.clone();
    let spinner = Spinner::new(MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER);
    let contracts_config = deploy_l1(
        shell,
        &forge_args,
        ecosystem_config,
        initial_deployment_config,
        &l1_rpc_url,
        None,
        true,
        support_l2_legacy_shared_bridge_test,
        bridgehub_address,
        zksync_os,
        reuse_gov_and_admin,
    )
    .await?;
    spinner.finish();

    accept_owner(
        shell,
        ecosystem_config.path_to_foundry_scripts(),
        contracts_config.l1.governance_addr,
        &ecosystem_config.get_wallets()?.governor,
        contracts_config
            .ecosystem_contracts
            .state_transition_proxy_addr,
        &forge_args,
        l1_rpc_url.clone(),
    )
    .await?;

    accept_admin(
        shell,
        ecosystem_config.path_to_foundry_scripts(),
        contracts_config.l1.chain_admin_addr,
        &ecosystem_config.get_wallets()?.governor,
        contracts_config
            .ecosystem_contracts
            .state_transition_proxy_addr,
        &forge_args,
        l1_rpc_url.clone(),
    )
    .await?;

    Ok(contracts_config)
}
