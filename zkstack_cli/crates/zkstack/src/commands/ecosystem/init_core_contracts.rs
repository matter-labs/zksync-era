use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{forge::ForgeScriptArgs, logger, spinner::Spinner};
use zkstack_cli_config::{
    forge_interface::deploy_ecosystem::input::InitialDeploymentConfig,
    traits::SaveConfigWithBasePath, CoreContractsConfig, EcosystemConfig, ZkStackConfig,
};

use crate::{
    admin_functions::{accept_admin, accept_owner},
    commands::ecosystem::{
        args::init::{InitCoreContractsArgs, InitCoreContractsArgsFinal},
        common::{deploy_erc20, deploy_l1_core_contracts},
        create_configs::{create_erc20_deployment_config, create_initial_deployments_config},
    },
    messages::{
        MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER, MSG_DEPLOYING_ERC20, MSG_INITIALIZING_ECOSYSTEM,
        MSG_INTALLING_DEPS_SPINNER,
    },
};

pub async fn run(args: InitCoreContractsArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;
    let zksync_os = args.common.zksync_os;

    let initial_deployment_config = match ecosystem_config.get_initial_deployment_config() {
        Ok(config) => config,
        Err(_) => create_initial_deployments_config(shell, &ecosystem_config.config)?,
    };

    let mut final_ecosystem_args = args
        .clone()
        .fill_values_with_prompt(ecosystem_config.l1_network)
        .await?;

    logger::info(MSG_INITIALIZING_ECOSYSTEM);

    let contracts_config = init_ecosystem(
        &mut final_ecosystem_args,
        shell,
        &ecosystem_config,
        &initial_deployment_config,
        zksync_os,
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
            final_ecosystem_args.l1_rpc_url.clone(),
        )
        .await?;
    }

    Ok(())
}

async fn init_ecosystem(
    init_args: &mut InitCoreContractsArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
    zksync_os: bool,
) -> anyhow::Result<CoreContractsConfig> {
    let spinner = Spinner::new(MSG_INTALLING_DEPS_SPINNER);
    spinner.finish();

    let contracts = deploy_ecosystem(
        shell,
        init_args.l1_rpc_url.clone(),
        init_args.forge_args.clone(),
        ecosystem_config,
        initial_deployment_config,
        init_args.support_l2_legacy_shared_bridge_test,
        zksync_os,
    )
    .await?;
    contracts.save_with_base_path(shell, &ecosystem_config.config)?;
    Ok(contracts)
}

pub async fn deploy_ecosystem(
    shell: &Shell,
    l1_rpc_url: String,
    forge_args: ForgeScriptArgs,
    ecosystem_config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
    support_l2_legacy_shared_bridge_test: bool,
    zksync_os: bool,
) -> anyhow::Result<CoreContractsConfig> {
    let spinner = Spinner::new(MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER);
    let contracts_config = deploy_l1_core_contracts(
        shell,
        &forge_args,
        ecosystem_config,
        initial_deployment_config,
        &l1_rpc_url,
        None,
        true,
        support_l2_legacy_shared_bridge_test,
        zksync_os,
    )
    .await?;
    spinner.finish();

    accept_owner(
        shell,
        ecosystem_config.path_to_foundry_scripts_for_ctm(zksync_os),
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
        ecosystem_config.path_to_foundry_scripts_for_ctm(zksync_os),
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
        ecosystem_config.path_to_foundry_scripts_for_ctm(zksync_os),
        contracts_config.l1.governance_addr,
        &ecosystem_config.get_wallets()?.governor,
        contracts_config.bridges.shared.l1_address,
        &forge_args,
        l1_rpc_url.clone(),
    )
    .await?;

    accept_owner(
        shell,
        ecosystem_config.path_to_foundry_scripts_for_ctm(zksync_os),
        contracts_config.l1.governance_addr,
        &ecosystem_config.get_wallets()?.governor,
        contracts_config
            .core_ecosystem_contracts
            .stm_deployment_tracker_proxy_addr
            .context("stm_deployment_tracker_proxy_addr")?,
        &forge_args,
        l1_rpc_url,
    )
    .await?;

    Ok(contracts_config)
}
