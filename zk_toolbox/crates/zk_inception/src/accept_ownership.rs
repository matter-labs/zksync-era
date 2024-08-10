use common::{
    forge::{Forge, ForgeScript, ForgeScriptArgs},
    spinner::Spinner,
};
use config::{forge_interface::script_params::ACCEPT_GOVERNANCE_SCRIPT_PARAMS, EcosystemConfig};
use ethers::{
    abi::parse_abi,
    contract::BaseContract,
    types::{Address, H256},
};
use lazy_static::lazy_static;
use xshell::Shell;

use crate::{
    messages::MSG_ACCEPTING_GOVERNANCE_SPINNER,
    utils::forge::{check_the_balance, fill_forge_private_key},
};

lazy_static! {
    static ref ACCEPT_ADMIN: BaseContract = BaseContract::from(
        parse_abi(&[
            "function acceptOwner(address governor, address target) public",
            "function acceptAdmin(address admin, address target) public",
            "function setDAValidatorPair(address admin, address diamondProxyAddress, address l1DAValidatorAddress, address l2DAValidatorAddress) public"
        ])
        .unwrap(),
    );
}

pub async fn accept_admin(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    admin: Address,
    governor: Option<H256>,
    target_address: Address,
    forge_args: &ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    // Resume for accept admin doesn't work properly. Foundry assumes that if signature of the function is the same,
    // than it's the same call, but because we are calling this function multiple times during the init process,
    // code assumes that doing only once is enough, but actually we need to accept admin multiple times
    let mut forge_args = forge_args.clone();
    forge_args.resume = false;

    let calldata = ACCEPT_ADMIN
        .encode("acceptAdmin", (admin, target_address))
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
    accept_ownership(shell, governor, forge).await
}

pub async fn accept_owner(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    governor_contract: Address,
    governor: Option<H256>,
    target_address: Address,
    forge_args: &ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    // resume doesn't properly work here.
    let mut forge_args = forge_args.clone();
    forge_args.resume = false;

    let calldata = ACCEPT_ADMIN
        .encode("acceptOwner", (governor_contract, target_address))
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
    accept_ownership(shell, governor, forge).await
}

pub async fn set_da_validator_pair(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    governor_contract: Address,
    governor: Option<H256>,
    diamond_proxy_address: Address,
    l1_da_validator_address: Address,
    l2_da_validator_address: Address,
    forge_args: &ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    // resume doesn't properly work here.
    let mut forge_args = forge_args.clone();
    forge_args.resume = false;

    let calldata = ACCEPT_ADMIN
        .encode("setDAValidatorPair", (governor_contract, diamond_proxy_address, l1_da_validator_address, l2_da_validator_address))
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
    accept_ownership(shell, governor, forge).await
}

async fn accept_ownership(
    shell: &Shell,
    governor: Option<H256>,
    mut forge: ForgeScript,
) -> anyhow::Result<()> {
    forge = fill_forge_private_key(forge, governor)?;
    check_the_balance(&forge).await?;
    let spinner = Spinner::new(MSG_ACCEPTING_GOVERNANCE_SPINNER);
    forge.run(shell)?;
    spinner.finish();
    Ok(())
}
