use std::{path::PathBuf, str::FromStr};

use anyhow::Context;
use common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    git, logger,
    spinner::Spinner,
    Prompt,
};
use config::{
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
use types::L1Network;
use xshell::Shell;

use super::{
    args::init::{EcosystemArgsFinal, EcosystemInitArgs, EcosystemInitArgsFinal},
    common::deploy_l1,
    setup_observability,
    utils::{build_system_contracts, install_yarn_dependencies},
};
use crate::{
    accept_ownership::{accept_admin, accept_owner},
    commands::{
        chain::{self, args::init::PortOffset},
        ecosystem::create_configs::{
            create_erc20_deployment_config, create_initial_deployments_config,
        },
    },
    messages::{
        msg_ecosystem_initialized, msg_ecosystem_no_found_preexisting_contract,
        msg_initializing_chain, MSG_CHAIN_NOT_INITIALIZED,
        MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER, MSG_DEPLOYING_ERC20,
        MSG_DEPLOYING_ERC20_SPINNER, MSG_ECOSYSTEM_CONTRACTS_PATH_INVALID_ERR,
        MSG_ECOSYSTEM_CONTRACTS_PATH_PROMPT, MSG_INITIALIZING_ECOSYSTEM,
        MSG_INTALLING_DEPS_SPINNER,
    },
    utils::forge::{check_the_balance, fill_forge_private_key},
};

pub async fn run(args: EcosystemInitArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    git::submodule_update(shell, ecosystem_config.link_to_code.clone())?;

    let initial_deployment_config = match ecosystem_config.get_initial_deployment_config() {
        Ok(config) => config,
        Err(_) => create_initial_deployments_config(shell, &ecosystem_config.config)?,
    };

    let mut genesis_args = args.genesis_args.clone();
    if args.dev {
        genesis_args.use_default = true;
    }
    let mut final_ecosystem_args = args.fill_values_with_prompt(ecosystem_config.l1_network);

    logger::info(MSG_INITIALIZING_ECOSYSTEM);

    if final_ecosystem_args.observability {
        setup_observability::run(shell)?;
    }

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

    // If the name of chain passed then we deploy exactly this chain otherwise deploy all chains
    let list_of_chains = if let Some(name) = global_config().chain_name.clone() {
        vec![name]
    } else {
        ecosystem_config.list_of_chains()
    };

    for chain_name in &list_of_chains {
        logger::info(msg_initializing_chain(chain_name));
        let chain_config = ecosystem_config
            .load_chain(Some(chain_name.clone()))
            .context(MSG_CHAIN_NOT_INITIALIZED)?;

        let mut chain_init_args = chain::args::init::InitArgsFinal {
            forge_args: final_ecosystem_args.forge_args.clone(),
            genesis_args: genesis_args.clone().fill_values_with_prompt(&chain_config),
            deploy_paymaster: final_ecosystem_args.deploy_paymaster,
            l1_rpc_url: final_ecosystem_args.ecosystem.l1_rpc_url.clone(),
            port_offset: PortOffset::from_chain_id(chain_config.id as u16).into(),
        };

        chain::init::init(
            &mut chain_init_args,
            shell,
            &ecosystem_config,
            &chain_config,
        )
        .await?;
    }

    logger::outro(msg_ecosystem_initialized(&list_of_chains.join(",")));

    Ok(())
}

async fn init(
    init_args: &mut EcosystemInitArgsFinal,
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
    let spinner = Spinner::new(MSG_DEPLOYING_ECOSYSTEM_CONTRACTS_SPINNER);
    let contracts_config = deploy_l1(
        shell,
        &forge_args,
        config,
        initial_deployment_config,
        &l1_rpc_url,
        None,
        true,
    )
    .await?;
    spinner.finish();

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

    accept_admin(
        shell,
        config,
        contracts_config.l1.chain_admin_addr,
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

    accept_admin(
        shell,
        config,
        contracts_config.l1.chain_admin_addr,
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

    accept_admin(
        shell,
        config,
        contracts_config.l1.chain_admin_addr,
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
