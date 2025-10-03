use xshell::Shell;
use zkstack_cli_common::{forge::ForgeScriptArgs, git, logger};
use zkstack_cli_config::{EcosystemConfig, ZkStackConfig, ZkStackConfigTrait};

use super::{
    args::init::{RegisterCTMArgs, RegisterCTMArgsFinal},
    common::register_ctm_on_existing_bh,
};
use crate::{commands::chain::utils::display_admin_script_output, messages::MSG_REGISTERING_CTM};

pub async fn run(args: RegisterCTMArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;

    if args.update_submodules.is_none() || args.update_submodules == Some(true) {
        git::submodule_update(shell, &ecosystem_config.link_to_code())?;
    }

    let mut final_ecosystem_args = args
        .clone()
        .fill_values_with_prompt(ecosystem_config.l1_network)
        .await?;

    logger::info(MSG_REGISTERING_CTM);

    let forge_args = final_ecosystem_args.forge_args.clone();

    register_ctm(
        &mut final_ecosystem_args,
        shell,
        forge_args,
        &ecosystem_config,
        args.only_save_calldata,
    )
    .await?;

    Ok(())
}

pub async fn register_ctm(
    init_args: &mut RegisterCTMArgsFinal,
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    ecosystem_config: &EcosystemConfig,
    only_save_calldata: bool,
) -> anyhow::Result<()> {
    let output = register_ctm_on_existing_bh(
        shell,
        &forge_args,
        ecosystem_config,
        &init_args.ecosystem.l1_rpc_url,
        None,
        init_args.bridgehub_address,
        init_args.ctm_address,
        only_save_calldata,
    )
    .await?;

    if only_save_calldata {
        display_admin_script_output(output);
    }
    Ok(())
}
