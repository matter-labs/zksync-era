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
    common::{deploy_l1, register_ctm_on_existing_bh},
    setup_observability,
    utils::{build_da_contracts, install_yarn_dependencies},
};
use crate::{
    admin_functions::{accept_admin, accept_owner},
    commands::{
        chain::{self},
        ecosystem::{
            args::init::PromptPolicy,
            create_configs::{create_erc20_deployment_config, create_initial_deployments_config},
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

    let prompt_policy = PromptPolicy {
        deploy_erc20: false,
        observability: true,
        skip_ecosystem: true,
    };

    let mut final_ecosystem_args = args
        .clone()
        .fill_values_with_prompt(ecosystem_config.l1_network, prompt_policy);

    logger::info(MSG_INITIALIZING_ECOSYSTEM);

    if final_ecosystem_args.observability {
        setup_observability::run(shell)?;
    }

    let forge_args = final_ecosystem_args.forge_args.clone();

    register_ctm(
        &mut final_ecosystem_args,
        shell,
        forge_args,
        &ecosystem_config,
        &initial_deployment_config,
    )
    .await?;

    // Initialize chain(s)
    let mut chains: Vec<String> = vec![];
    if !final_ecosystem_args.ecosystem_only {
        chains = init_chains(&args, &final_ecosystem_args, shell, &ecosystem_config).await?;
    }
    logger::outro(msg_ecosystem_initialized(&chains.join(",")));

    Ok(())
}

pub async fn register_ctm(
    init_args: &mut EcosystemInitArgsFinal,
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    ecosystem_config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
) -> anyhow::Result<()> {
    register_ctm_inner_inner(
        shell,
        forge_args,
        ecosystem_config,
        initial_deployment_config,
        init_args.ecosystem.l1_rpc_url.clone(),
    )
    .await?;

    Ok(())
}

async fn register_ctm_inner_inner(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    config: &EcosystemConfig,
    initial_deployment_config: &InitialDeploymentConfig,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    register_ctm_on_existing_bh(
        shell,
        &forge_args,
        config,
        initial_deployment_config,
        &l1_rpc_url,
        None,
        true,
    )
    .await?;

    Ok(())
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
