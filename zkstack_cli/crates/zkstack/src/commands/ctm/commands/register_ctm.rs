use xshell::Shell;
use zkstack_cli_common::{
    forge::{Forge, ForgeScriptArgs},
    git, logger,
};
use zkstack_cli_config::{
    forge_interface::script_params::REGISTER_CTM_SCRIPT_PARAMS, EcosystemConfig, ZkStackConfig,
    ZkStackConfigTrait,
};
use zkstack_cli_types::L1Network;

use crate::{
    commands::ctm::args::{RegisterCTMArgs, RegisterCTMArgsFinal},
    messages::MSG_REGISTERING_CTM,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

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
    register_ctm_on_existing_bh(
        shell,
        &forge_args,
        ecosystem_config,
        &init_args.ecosystem.l1_rpc_url,
        None,
        true,
    )
    .await?;

    Ok(())
}

pub async fn register_ctm_on_existing_bh(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    config: &EcosystemConfig,
    l1_rpc_url: &str,
    sender: Option<String>,
    broadcast: bool,
) -> anyhow::Result<()> {
    let wallets_config = config.get_wallets()?;

    let mut forge = Forge::new(&config.path_to_l1_foundry())
        .script(&REGISTER_CTM_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url.to_string());

    if config.l1_network == L1Network::Localhost {
        // It's a kludge for reth, just because it doesn't behave properly with large amount of txs
        forge = forge.with_slow();
    }

    if let Some(address) = sender {
        forge = forge.with_sender(address);
    } else {
        forge =
            fill_forge_private_key(forge, Some(&wallets_config.governor), WalletOwner::Governor)?;
    }

    if broadcast {
        forge = forge.with_broadcast();
        check_the_balance(&forge).await?;
    }

    forge.run(shell)?;

    Ok(())
}
