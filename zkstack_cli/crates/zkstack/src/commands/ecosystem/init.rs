use std::{path::PathBuf, str::FromStr};

use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config,
    contracts::{build_l1_contracts, build_l2_contracts, build_system_contracts},
    forge::{Forge, ForgeScriptArgs},
    git, logger,
    spinner::Spinner,
    Prompt,
};
use zkstack_cli_config::{
    forge_interface::{
        deploy_ecosystem::{
            input::{DeployErc20Config, Erc20DeploymentConfig, InitialDeploymentConfig},
            output::ERC20Tokens,
        },
        script_params::DEPLOY_ERC20_SCRIPT_PARAMS,
    },
    traits::{FileConfigWithDefaultName, ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ContractsConfig, EcosystemConfig,
};
use zkstack_cli_types::L1Network;

use super::{
    args::init::{EcosystemArgsFinal, EcosystemInitArgs, EcosystemInitArgsFinal},
    common::deploy_l1,
    setup_observability,
    utils::{build_da_contracts, install_yarn_dependencies},
};
use crate::{
    admin_functions::{accept_admin, accept_owner},
    commands::{
        chain::{self},
        ecosystem::create_configs::{
            create_erc20_deployment_config, create_initial_deployments_config,
        },
    },
    messages::{
        msg_chain_load_err, msg_ecosystem_initialized, msg_ecosystem_no_found_preexisting_contract,
        msg_initializing_chain, MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER, MSG_DEPLOYING_ERC20,
        MSG_DEPLOYING_ERC20_SPINNER, MSG_ECOSYSTEM_CONTRACTS_PATH_INVALID_ERR,
        MSG_ECOSYSTEM_CONTRACTS_PATH_PROMPT, MSG_INITIALIZING_ECOSYSTEM,
        MSG_INTALLING_DEPS_SPINNER,
    },
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
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

    let mut final_ecosystem_args = args
        .clone()
        .fill_values_with_prompt(ecosystem_config.l1_network);

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

    // Initialize chain(s)
    let mut chains: Vec<String> = vec![];
    if !final_ecosystem_args.ecosystem_only {
        chains = init_chains(&args, &final_ecosystem_args, shell, &ecosystem_config).await?;
    }
    logger::outro(msg_ecosystem_initialized(&chains.join(",")));

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

async fn deploy_erc20(
    shell: &Shell,
    erc20_deployment_config: &Erc20DeploymentConfig,
    ecosystem_config: &EcosystemConfig,
    contracts_config: &ContractsConfig,
    forge_args: ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<ERC20Tokens> {
    let deploy_config_path =
        DEPLOY_ERC20_SCRIPT_PARAMS.input(&ecosystem_config.path_to_l1_foundry());
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

    let mut forge = Forge::new(&ecosystem_config.path_to_l1_foundry())
        .script(&DEPLOY_ERC20_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast();

    forge = fill_forge_private_key(
        forge,
        ecosystem_config.get_wallets()?.deployer.as_ref(),
        WalletOwner::Deployer,
    )?;

    let spinner = Spinner::new(MSG_DEPLOYING_ERC20_SPINNER);
    check_the_balance(&forge).await?;
    forge.run(shell)?;
    spinner.finish();

    let result = ERC20Tokens::read(
        shell,
        DEPLOY_ERC20_SCRIPT_PARAMS.output(&ecosystem_config.path_to_l1_foundry()),
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
    let contracts_config = deploy_l1(
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

    // Note, that there is no admin in L1 asset router, so we do
    // need to accept it

    accept_owner(
        shell,
        config,
        contracts_config.l1.governance_addr,
        &config.get_wallets()?.governor,
        contracts_config
            .ecosystem_contracts
            .state_transition_proxy_addr,
        &forge_args,
        l1_rpc_url.clone(),
    )
    .await?;

    accept_admin(
        shell,
        config,
        contracts_config.l1.chain_admin_addr,
        &config.get_wallets()?.governor,
        contracts_config
            .ecosystem_contracts
            .state_transition_proxy_addr,
        &forge_args,
        l1_rpc_url.clone(),
    )
    .await?;

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

    Ok(contracts_config)
}

async fn init_chains(
    init_args: &EcosystemInitArgs,
    final_init_args: &EcosystemInitArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<Vec<String>> {
    // If the name of chain passed then we deploy exactly this chain otherwise deploy all chains
    let list_of_chains = if let Some(name) = global_config().chain_name.clone() {
        vec![name]
    } else {
        ecosystem_config.list_of_chains()
    };
    // Set default values for dev mode
    let mut deploy_paymaster = init_args.deploy_paymaster;
    let mut genesis_args = init_args.get_genesis_args().clone();
    if final_init_args.dev {
        deploy_paymaster = Some(true);
        genesis_args.dev = true;
    }
    // Can't initialize multiple chains with the same DB
    if list_of_chains.len() > 1 {
        genesis_args.reset_db_names();
    }
    // Initialize chains
    for chain_name in &list_of_chains {
        logger::info(msg_initializing_chain(chain_name));
        let chain_config = ecosystem_config
            .load_chain(Some(chain_name.clone()))
            .context(msg_chain_load_err(chain_name))?;

        let chain_init_args = chain::args::init::InitArgs {
            forge_args: final_init_args.forge_args.clone(),
            server_db_url: genesis_args.server_db_url.clone(),
            server_db_name: genesis_args.server_db_name.clone(),
            dont_drop: genesis_args.dont_drop,
            deploy_paymaster,
            l1_rpc_url: Some(final_init_args.ecosystem.l1_rpc_url.clone()),
            no_port_reallocation: final_init_args.no_port_reallocation,
            update_submodules: init_args.update_submodules,
            dev: final_init_args.dev,
            validium_args: final_init_args.validium_args.clone(),
            server_command: genesis_args.server_command.clone(),
        };
        let final_chain_init_args = chain_init_args.fill_values_with_prompt(&chain_config);

        chain::init::init(
            &final_chain_init_args,
            shell,
            ecosystem_config,
            &chain_config,
        )
        .await?;
    }
    Ok(list_of_chains)
}
