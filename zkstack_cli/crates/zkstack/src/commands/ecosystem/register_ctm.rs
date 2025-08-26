use xshell::Shell;
use zkstack_cli_common::{forge::ForgeScriptArgs, git, logger};
use zkstack_cli_config::EcosystemConfig;

use super::{
    args::init::{RegisterCTMArgs, RegisterCTMArgsFinal},
    common::register_ctm_on_existing_bh,
};
use crate::{commands::ecosystem::args::init::PromptPolicy, messages::MSG_REGISTERING_CTM};

pub async fn run(args: RegisterCTMArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    if args.update_submodules.is_none() || args.update_submodules == Some(true) {
        git::submodule_update(shell, ecosystem_config.link_to_code.clone())?;
    }

    let prompt_policy = PromptPolicy {
        deploy_erc20: false,
        observability: true,
        skip_ecosystem: true,
    };

    let mut final_ecosystem_args = args
        .clone()
        .fill_values_with_prompt(ecosystem_config.l1_network, prompt_policy)
        .await?;

    logger::info(MSG_REGISTERING_CTM);

    let forge_args = final_ecosystem_args.forge_args.clone();

    register_ctm(
        &mut final_ecosystem_args,
        shell,
        forge_args,
        &ecosystem_config,
    )
    .await?;

    Ok(())
}

pub async fn register_ctm(
    init_args: &mut RegisterCTMArgsFinal,
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<()> {
    register_ctm_inner_inner(
        shell,
        forge_args,
        ecosystem_config,
        init_args.ecosystem.l1_rpc_url.clone(),
    )
    .await?;

    Ok(())
}

async fn register_ctm_inner_inner(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    config: &EcosystemConfig,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    register_ctm_on_existing_bh(shell, &forge_args, config, &l1_rpc_url, None, true).await?;

    Ok(())
}
