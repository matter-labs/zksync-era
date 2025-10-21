use ethers::{abi::parse_abi, contract::BaseContract};
use lazy_static::lazy_static;
use xshell::Shell;
use zkstack_cli_common::{
    forge::{Forge, ForgeRunner, ForgeScriptArgs},
    logger,
};
use zkstack_cli_config::{
    forge_interface::script_params::REGISTER_CTM_SCRIPT_PARAMS, traits::ReadConfig,
    EcosystemConfig, ZkStackConfig,
};
use zkstack_cli_types::{L1Network, VMOption};
use zksync_types::H160;

use crate::{
    admin_functions::{AdminScriptOutput, AdminScriptOutputInner},
    commands::{
        chain::utils::display_admin_script_output,
        ecosystem::args::register_new_ctm::RegisterCTMArgs,
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
    let vm_option = args.common.vm_option();

    let final_ecosystem_args = args
        .clone()
        .fill_values_with_prompt(ecosystem_config.l1_network)
        .await?;

    logger::info(MSG_REGISTERING_CTM);

    let bridgehub_address = if let Some(addr) = final_ecosystem_args.bridgehub {
        addr
    } else {
        ecosystem_config
            .get_contracts_config()?
            .core_ecosystem_contracts
            .bridgehub_proxy_addr
    };

    let ctm_address = if let Some(addr) = final_ecosystem_args.ctm {
        addr
    } else {
        ecosystem_config
            .get_contracts_config()?
            .ctm(vm_option)
            .state_transition_proxy_addr
    };

    let mut runner = ForgeRunner::new(final_ecosystem_args.forge_args.runner.clone());
    let output = register_ctm_on_existing_bh(
        shell,
        &mut runner,
        &final_ecosystem_args.forge_args.script,
        &ecosystem_config,
        &final_ecosystem_args.l1_rpc_url,
        None,
        bridgehub_address,
        ctm_address,
        final_ecosystem_args.only_save_calldata,
        vm_option,
    )
    .await?;

    if final_ecosystem_args.only_save_calldata {
        display_admin_script_output(output);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn register_ctm_on_existing_bh(
    shell: &Shell,
    runner: &mut ForgeRunner,
    forge_args: &ForgeScriptArgs,
    config: &EcosystemConfig,
    l1_rpc_url: &str,
    sender: Option<String>,
    bridgehub_address: H160,
    ctm_address: H160,
    only_save_calldata: bool,
    vm_option: VMOption,
) -> anyhow::Result<AdminScriptOutput> {
    let wallets_config = config.get_wallets()?;

    let calldata = REGISTER_CTM_FUNCTIONS
        .encode(
            "registerCTM",
            (bridgehub_address, ctm_address, !only_save_calldata),
        )
        .unwrap();

    let mut forge = Forge::new(&config.path_to_foundry_scripts_for_ctm(vm_option))
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

    let output_path =
        REGISTER_CTM_SCRIPT_PARAMS.output(&config.path_to_foundry_scripts_for_ctm(vm_option));
    runner.run(shell, forge)?;

    Ok(AdminScriptOutputInner::read(shell, output_path)?.into())
}
