use anyhow::Context;
use common::{
    config::global_config,
    forge::{Forge, ForgeScript, ForgeScriptArgs},
    logger,
    spinner::Spinner,
};
use config::{forge_interface::script_params::ACCEPT_GOVERNANCE_SCRIPT_PARAMS, EcosystemConfig};
use ethers::{abi::parse_abi, contract::BaseContract, utils::hex};
use lazy_static::lazy_static;
use xshell::Shell;
use zksync_basic_types::{Address, H256};

use crate::{
    messages::{
        MSG_CHAIN_NOT_INITIALIZED, MSG_L1_SECRETS_MUST_BE_PRESENTED,
        MSG_TOKEN_MULTIPLIER_SETTER_UPDATED_TO, MSG_UPDATING_TOKEN_MULTIPLIER_SETTER_SPINNER,
        MSG_WALLETS_CONFIG_MUST_BE_PRESENT,
    },
    utils::forge::{check_the_balance, fill_forge_private_key},
};

lazy_static! {
    static ref SET_TOKEN_MULTIPLIER_SETTER: BaseContract = BaseContract::from(
        parse_abi(&[
            "function chainSetTokenMultiplierSetter(address chainAdmin, address target) public"
        ])
        .unwrap(),
    );
}

pub async fn run(args: ForgeScriptArgs, shell: &Shell) -> anyhow::Result<()> {
    let chain_name = global_config().chain_name.clone();
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;
    let contracts_config = chain_config.get_contracts_config()?;
    let l1_url = chain_config
        .get_secrets_config()?
        .l1
        .context(MSG_L1_SECRETS_MUST_BE_PRESENTED)?
        .l1_rpc_url
        .expose_str()
        .to_string();
    let token_multiplier_setter_address = ecosystem_config
        .get_wallets()
        .context(MSG_WALLETS_CONFIG_MUST_BE_PRESENT)?
        .token_multiplier_setter
        .address;

    let spinner = Spinner::new(MSG_UPDATING_TOKEN_MULTIPLIER_SETTER_SPINNER);
    set_token_multiplier_setter(
        shell,
        &ecosystem_config,
        chain_config.get_wallets_config()?.governor_private_key(),
        contracts_config.l1.chain_admin_addr,
        token_multiplier_setter_address,
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

pub async fn set_token_multiplier_setter(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    governor: Option<H256>,
    chain_admin_address: Address,
    target_address: Address,
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
            (chain_admin_address, target_address),
        )
        .unwrap();
    let foundry_contracts_path = ecosystem_config.path_to_foundry();
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
    governor: Option<H256>,
    mut forge: ForgeScript,
) -> anyhow::Result<()> {
    forge = fill_forge_private_key(forge, governor)?;
    check_the_balance(&forge).await?;
    forge.run(shell)?;
    Ok(())
}
