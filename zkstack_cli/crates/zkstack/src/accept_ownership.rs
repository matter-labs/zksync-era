use anyhow::Context;
use common::{
    forge::{Forge, ForgeScript, ForgeScriptArgs},
    spinner::Spinner,
    wallets::Wallet,
};
use config::{
    forge_interface::script_params::ACCEPT_GOVERNANCE_SCRIPT_PARAMS, ChainConfig, ContractsConfig,
    EcosystemConfig,
};
use ethers::{
    abi::{parse_abi, Token},
    contract::BaseContract,
    types::Address,
};
use lazy_static::lazy_static;
use xshell::Shell;
use zksync_types::U256;

use crate::{
    messages::MSG_ACCEPTING_GOVERNANCE_SPINNER,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

lazy_static! {
    static ref ACCEPT_ADMIN: BaseContract = BaseContract::from(
        parse_abi(&[
            "function governanceAcceptOwner(address governor, address target) public",
            "function chainAdminAcceptAdmin(address admin, address target) public",
            "function setDAValidatorPair(address chainAdmin, address target, address l1DaValidator, address l2DaValidator) public",
            "function makePermanentRollup(address chainAdmin, address target) public",
            "function governanceExecuteCalls(bytes calldata callsToExecute, address target) public",
            "function adminExecuteUpgrade(bytes memory diamondCut, address adminAddr, address accessControlRestriction, address chainDiamondProxy)",
            "function adminScheduleUpgrade(address adminAddr, address accessControlRestriction, uint256 newProtocolVersion, uint256 timestamp)",
            "function updateValidator(address adminAddr,address accessControlRestriction,address validatorTimelock,uint256 chainId,address validatorAddress,bool addValidator) public"
        ])
        .unwrap(),
    );
}

pub async fn accept_admin(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    admin: Address,
    governor: &Wallet,
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
        .encode("chainAdminAcceptAdmin", (admin, target_address))
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
    accept_ownership(shell, governor, forge).await
}

pub async fn accept_owner(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    governor_contract: Address,
    governor: &Wallet,
    target_address: Address,
    forge_args: &ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    // resume doesn't properly work here.
    let mut forge_args = forge_args.clone();
    forge_args.resume = false;

    let calldata = ACCEPT_ADMIN
        .encode("governanceAcceptOwner", (governor_contract, target_address))
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
    accept_ownership(shell, governor, forge).await
}

#[allow(clippy::too_many_arguments)]
pub async fn set_da_validator_pair(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    chain_admin_addr: Address,
    governor: &Wallet,
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
        .encode(
            "setDAValidatorPair",
            (
                chain_admin_addr,
                diamond_proxy_address,
                l1_da_validator_address,
                l2_da_validator_address,
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
    accept_ownership(shell, governor, forge).await
}

#[allow(clippy::too_many_arguments)]
pub async fn make_permanent_rollup(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    chain_admin_addr: Address,
    governor: &Wallet,
    diamond_proxy_address: Address,
    forge_args: &ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    // resume doesn't properly work here.
    let mut forge_args = forge_args.clone();
    forge_args.resume = false;

    let calldata = ACCEPT_ADMIN
        .encode(
            "makePermanentRollup",
            (chain_admin_addr, diamond_proxy_address),
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
    accept_ownership(shell, governor, forge).await
}

#[allow(clippy::too_many_arguments)]
pub async fn governance_execute_calls(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    governor: &Wallet,
    encoded_calls: Vec<u8>,
    forge_args: &ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    // resume doesn't properly work here.
    let mut forge_args = forge_args.clone();
    forge_args.resume = false;

    let governance_address = ecosystem_config.get_contracts_config()?.l1.governance_addr;

    let calldata = ACCEPT_ADMIN
        .encode(
            "governanceExecuteCalls",
            (Token::Bytes(encoded_calls), governance_address),
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
    accept_ownership(shell, governor, forge).await
}

#[allow(clippy::too_many_arguments)]
pub async fn admin_execute_upgrade(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    chain_contracts_config: &ContractsConfig,
    governor: &Wallet,
    upgrade_diamond_cut: Vec<u8>,
    forge_args: &ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    // resume doesn't properly work here.
    let mut forge_args = forge_args.clone();
    forge_args.resume = false;

    let admin_addr = chain_contracts_config.l1.chain_admin_addr;
    let access_control_restriction = chain_contracts_config
        .l1
        .access_control_restriction_addr
        .context("no access_control_restriction_addr")?;
    let diamond_proxy = chain_contracts_config.l1.diamond_proxy_addr;

    let calldata = ACCEPT_ADMIN
        .encode(
            "adminExecuteUpgrade",
            (
                Token::Bytes(upgrade_diamond_cut),
                admin_addr,
                access_control_restriction,
                diamond_proxy,
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
    accept_ownership(shell, governor, forge).await
}

#[allow(clippy::too_many_arguments)]
pub async fn admin_schedule_upgrade(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    chain_contracts_config: &ContractsConfig,
    new_protocol_version: U256,
    timestamp: U256,
    governor: &Wallet,
    forge_args: &ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    // resume doesn't properly work here.
    let mut forge_args = forge_args.clone();
    forge_args.resume = false;

    let admin_addr = chain_contracts_config.l1.chain_admin_addr;
    let access_control_restriction = chain_contracts_config
        .l1
        .access_control_restriction_addr
        .context("no access_control_restriction_addr")?;

    let calldata = ACCEPT_ADMIN
        .encode(
            "adminScheduleUpgrade",
            (
                admin_addr,
                access_control_restriction,
                new_protocol_version,
                timestamp,
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
    accept_ownership(shell, governor, forge).await
}

#[allow(clippy::too_many_arguments)]
pub async fn admin_update_validator(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    chain_config: &ChainConfig,
    validator_timelock: Address,
    validator: Address,
    add_validator: bool,
    governor: &Wallet,
    forge_args: &ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    // resume doesn't properly work here.
    let mut forge_args = forge_args.clone();
    forge_args.resume = false;

    let chain_contracts_config = chain_config.get_contracts_config()?;

    let admin_addr = chain_contracts_config.l1.chain_admin_addr;
    let access_control_restriction = chain_contracts_config
        .l1
        .access_control_restriction_addr
        .context("no access_control_restriction_addr")?;

    let calldata = ACCEPT_ADMIN
        .encode(
            "updateValidator",
            (
                admin_addr,
                access_control_restriction,
                validator_timelock,
                chain_config.chain_id.as_u64(),
                validator,
                add_validator,
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
    accept_ownership(shell, governor, forge).await
}

async fn accept_ownership(
    shell: &Shell,
    governor: &Wallet,
    mut forge: ForgeScript,
) -> anyhow::Result<()> {
    forge = fill_forge_private_key(forge, Some(governor), WalletOwner::Governor)?;
    check_the_balance(&forge).await?;
    let spinner = Spinner::new(MSG_ACCEPTING_GOVERNANCE_SPINNER);
    forge.run(shell)?;
    spinner.finish();
    Ok(())
}
