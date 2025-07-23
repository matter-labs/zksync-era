use xshell::Shell;
use zkstack_cli_common::{forge::ForgeScriptArgs, git, logger};
use zkstack_cli_config::{
    forge_interface::deploy_ecosystem::input::InitialDeploymentConfig, EcosystemConfig,
};

use super::{
    args::init::{EcosystemInitArgs, EcosystemInitArgsFinal},
    common::{init_chains, register_ctm_on_existing_bh},
    setup_observability,
};
use crate::{
    commands::ecosystem::{
        args::init::PromptPolicy, create_configs::create_initial_deployments_config,
    },
    messages::{msg_ecosystem_initialized, MSG_INITIALIZING_ECOSYSTEM},
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
