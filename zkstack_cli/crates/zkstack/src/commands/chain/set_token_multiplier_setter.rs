use anyhow::Context;
use ethers::{abi::parse_abi, contract::BaseContract, utils::hex};
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
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_TOKEN_MULTIPLIER_SETTER_UPDATED_TO,
        MSG_UPDATING_TOKEN_MULTIPLIER_SETTER_SPINNER, MSG_WALLETS_CONFIG_MUST_BE_PRESENT,
        MSG_WALLET_TOKEN_MULTIPLIER_SETTER_NOT_FOUND,
    },
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

lazy_static! {
    static ref SET_TOKEN_MULTIPLIER_SETTER: BaseContract = BaseContract::from(
        parse_abi(&[
            "function chainSetTokenMultiplierSetter(address chainAdmin, address accessControlRestriction, address diamondProxyAddress, address setter) public"
        ])
        .unwrap(),
    );
}

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let contracts_config = chain_config.get_contracts_config()?;
    let l1_url = chain_config.get_secrets_config().await?.l1_rpc_url()?;
    let token_multiplier_setter_address = chain_config
        .get_wallets_config()
        .context(MSG_WALLETS_CONFIG_MUST_BE_PRESENT)?
        .token_multiplier_setter
        .context(MSG_WALLET_TOKEN_MULTIPLIER_SETTER_NOT_FOUND)?
        .address;

    let spinner = Spinner::new(MSG_UPDATING_TOKEN_MULTIPLIER_SETTER_SPINNER);
    set_token_multiplier_setter(
        shell,
        &ecosystem_config,
        &chain_config.get_wallets_config()?.governor,
        contracts_config
            .l1
            .access_control_restriction_addr
            .context("access_control_restriction_addr")?,
        contracts_config.l1.diamond_proxy_addr,
        token_multiplier_setter_address,
        contracts_config.l1.chain_admin_addr,
        &args.clone(),
        l1_url,
    )
    .await?;
    spinner.finish();

    logger::note(
        MSG_TOKEN_MULTIPLIER_SETTER_UPDATED_TO,
        hex::encode(token_multiplier_setter_address),
    );

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn set_token_multiplier_setter(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    governor: &Wallet,
    access_control_restriction_address: Address,
    diamond_proxy_address: Address,
    new_setter_address: Address,
    chain_admin_addr: Address,
    forge_args: &ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    // Resume for accept admin doesn't work properly. Foundry assumes that if signature of the function is the same,
    // then it's the same call, but because we are calling this function multiple times during the init process,
    // code assumes that doing only once is enough, but actually we need to accept admin multiple times
    let mut forge_args = forge_args.clone();
    forge_args.resume = false;

    let calldata = SET_TOKEN_MULTIPLIER_SETTER
        .encode(
            "chainSetTokenMultiplierSetter",
            (
                chain_admin_addr,
                access_control_restriction_address,
                diamond_proxy_address,
                new_setter_address,
            ),
        )
        .unwrap();
    let foundry_contracts_path = ecosystem_config.path_to_l1_foundry();
    let forge = Forge::new(&foundry_contracts_path)
        .script(
            &ACCEPT_GOVERNANCE_SCRIPT_PARAMS.script(),
            forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_broadcast()
        .with_calldata(&calldata);
    update_token_multiplier_setter(shell, governor, forge).await
}

async fn update_token_multiplier_setter(
    shell: &Shell,
    governor: &Wallet,
    mut forge: ForgeScript,
) -> anyhow::Result<()> {
    forge = fill_forge_private_key(forge, Some(governor), WalletOwner::Governor)?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;
    Ok(())
}
