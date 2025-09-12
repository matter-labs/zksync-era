use ethers::{abi::parse_abi, contract::BaseContract};
use lazy_static::lazy_static;
use xshell::Shell;
use zkstack_cli_common::{
    forge::{Forge, ForgeScriptArgs},
    git, logger,
};
use zkstack_cli_config::{
    forge_interface::script_params::REGISTER_CTM_SCRIPT_PARAMS, traits::ReadConfig,
    EcosystemConfig, ZkStackConfig, ZkStackConfigTrait,
};
use zkstack_cli_types::L1Network;
use zksync_types::H160;

use crate::{
    admin_functions::{AdminScriptOutput, AdminScriptOutputInner},
    commands::{
        chain::utils::display_admin_script_output,
        ctm::args::{RegisterCTMArgs, RegisterCTMArgsFinal},
    },
    messages::MSG_REGISTERING_CTM,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

lazy_static! {
   static ref REGISTER_CTM_FUNCTIONS: BaseContract =
        BaseContract::from(parse_abi(&["function registerCTM(address bridgehub, address chainTypeManagerProxy, bool shouldSend) public",]).unwrap(),);
}

pub async fn run(args: RegisterCTMArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;

    if args.update_submodules.is_none() || args.update_submodules == Some(true) {
        git::submodule_update(shell, &ecosystem_config.link_to_code())?;
    }

    let final_ecosystem_args = args
        .clone()
        .fill_values_with_prompt(ecosystem_config.l1_network)
        .await?;

    logger::info(MSG_REGISTERING_CTM);

    register_ctm(&final_ecosystem_args, shell, &ecosystem_config).await?;

    Ok(())
}

pub async fn register_ctm(
    init_args: &RegisterCTMArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
) -> anyhow::Result<()> {
    let output = register_ctm_on_existing_bh(
        shell,
        &init_args.forge_args,
        ecosystem_config,
        &init_args.ecosystem.l1_rpc_url,
        None,
        init_args.bridgehub_address,
        init_args.ctm_address,
        init_args.only_save_calldata,
    )
    .await?;

    if init_args.only_save_calldata {
        display_admin_script_output(output);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn register_ctm_on_existing_bh(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    config: &EcosystemConfig,
    l1_rpc_url: &str,
    sender: Option<String>,
    bridgehub_address: H160,
    ctm_address: H160,
    only_save_calldata: bool,
) -> anyhow::Result<AdminScriptOutput> {
    let wallets_config = config.get_wallets()?;

    let calldata = REGISTER_CTM_FUNCTIONS
        .encode(
            "registerCTM",
            (bridgehub_address, ctm_address, !only_save_calldata),
        )
        .unwrap();

    let mut forge = Forge::new(&config.path_to_foundry_scripts())
        .script(&REGISTER_CTM_SCRIPT_PARAMS.script(), forge_args.clone())
        .with_ffi()
        .with_calldata(&calldata)
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

    if !only_save_calldata {
        forge = forge.with_broadcast();
        check_the_balance(&forge).await?;
    }

    let output_path = REGISTER_CTM_SCRIPT_PARAMS.output(&config.path_to_foundry_scripts());
    forge.run(shell)?;

    Ok(AdminScriptOutputInner::read(shell, output_path)?.into())
}
