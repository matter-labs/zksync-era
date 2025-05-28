use anyhow::Context;
use ethers::{abi::parse_abi, contract::BaseContract};
use lazy_static::lazy_static;
use xshell::Shell;
use zkstack_cli_common::{
    forge::{Forge, ForgeScript, ForgeScriptArgs},
    logger,
    spinner::Spinner,
    wallets::Wallet,
};
use zkstack_cli_config::{
    forge_interface::script_params::ACCEPT_GOVERNANCE_SCRIPT_PARAMS, EcosystemConfig,
};
use zksync_basic_types::Address;

use crate::{
    commands::chain::args::set_pubdata_pricing_mode::SetPubdataPricingModeArgs,
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_PUBDATA_PRICING_MODE_UPDATED_TO,
        MSG_UPDATING_PUBDATA_PRICING_MODE_SPINNER,
    },
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

lazy_static! {
    static ref PUBDATA_PRICING_MODE_SETTER: BaseContract = BaseContract::from(
        parse_abi(&[
            "function setPubdataPricingMode(address chainAdmin, address target, uint8 pricingMode) public"
        ])
        .unwrap(),
    );
}

pub async fn run(args: SetPubdataPricingModeArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let contracts_config = chain_config.get_contracts_config()?;
    let l1_url = chain_config.get_secrets_config().await?.l1_rpc_url()?;
    let pubdata_pricing_mode: u8 = if args.rollup.unwrap() { 0 } else { 1 };

    let spinner = Spinner::new(MSG_UPDATING_PUBDATA_PRICING_MODE_SPINNER);
    set_pubdata_pricing_mode(
        shell,
        &ecosystem_config,
        &chain_config.get_wallets_config()?.governor,
        contracts_config.l1.chain_admin_addr,
        contracts_config.l1.diamond_proxy_addr,
        pubdata_pricing_mode,
        &mut args.forge_args.clone(),
        l1_url,
    )
    .await?;
    spinner.finish();
    logger::note(
        MSG_PUBDATA_PRICING_MODE_UPDATED_TO,
        pubdata_pricing_mode.to_string(),
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn set_pubdata_pricing_mode(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    governor: &Wallet,
    chain_admin_addr: Address,
    diamond_proxy_address: Address,
    pubdata_pricing_mode: u8,
    args: &mut ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    args.resume = false;

    let calldata = PUBDATA_PRICING_MODE_SETTER
        .encode(
            "setPubdataPricingMode",
            (
                chain_admin_addr,
                diamond_proxy_address,
                pubdata_pricing_mode,
            ),
        )
        .unwrap();
    let foundry_contracts_path = ecosystem_config.path_to_l1_foundry();
    let forge = Forge::new(&foundry_contracts_path)
        .script(&ACCEPT_GOVERNANCE_SCRIPT_PARAMS.script(), args.clone())
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast()
        .with_calldata(&calldata);
    update_pubdata_pricing_mode(shell, governor, forge).await
}

async fn update_pubdata_pricing_mode(
    shell: &Shell,
    governor: &Wallet,
    mut forge: ForgeScript,
) -> anyhow::Result<()> {
    forge = fill_forge_private_key(forge, Some(governor), WalletOwner::Governor)?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;
    Ok(())
}
