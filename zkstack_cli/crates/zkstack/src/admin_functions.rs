use std::path::Path;

use anyhow::Context;
use ethers::{
    abi::{parse_abi, Token},
    contract::BaseContract,
    types::{Address, Bytes},
    utils::hex,
};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::{
    forge::{Forge, ForgeScript, ForgeScriptArgs},
    spinner::Spinner,
    wallets::Wallet,
};
use zkstack_cli_config::{
    forge_interface::script_params::ACCEPT_GOVERNANCE_SCRIPT_PARAMS,
    traits::{ReadConfig, ZkStackConfig},
    ChainConfig, ContractsConfig, EcosystemConfig,
};
use zksync_basic_types::U256;

use crate::{
    commands::chain::admin_call_builder::{decode_admin_calls, AdminCall},
    messages::MSG_ACCEPTING_GOVERNANCE_SPINNER,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

lazy_static! {
    static ref ADMIN_FUNCTIONS: BaseContract = BaseContract::from(
        parse_abi(&[
            "function governanceAcceptOwner(address governor, address target) public",
            "function chainAdminAcceptAdmin(address admin, address target) public",
            "function setDAValidatorPair(address _bridgehub, uint256 _chainId, address _l1DaValidator, address _l2DaValidator, bool _shouldSend) public",
            "function setDAValidatorPairWithGateway(address bridgehub, uint256 l1GasPrice, uint256 l2ChainId, uint256 gatewayChainId, address l1DAValidator, address l2DAValidator, address chainDiamondProxyOnGateway, address refundRecipient, bool _shouldSend)",
            "function makePermanentRollup(address chainAdmin, address target) public",
            "function governanceExecuteCalls(bytes calldata callsToExecute, address target) public",
            "function adminExecuteUpgrade(bytes memory diamondCut, address adminAddr, address accessControlRestriction, address chainDiamondProxy)",
            "function adminScheduleUpgrade(address adminAddr, address accessControlRestriction, uint256 newProtocolVersion, uint256 timestamp)",
            "function updateValidator(address adminAddr,address accessControlRestriction,address validatorTimelock,uint256 chainId,address validatorAddress,bool addValidator) public",
            "function setTransactionFilterer(address _bridgehubAddr, uint256 _chainId, address _transactionFiltererAddress, bool _shouldSend) external",
            "function grantGatewayWhitelist(address _bridgehubAddr, uint256 _chainId, address[] calldata _grantee, bool _shouldSend)",
            "function migrateChainToGateway(address bridgehub, uint256 l1GasPrice, uint256 l2GhainId, uint256 gatewayChainId, bytes _gatewayDiamondCutData, address refundRecipient, bool _shouldSend) public view",
            "function revokeGatewayWhitelist(address _bridgehub, uint256 _chainId, address _address, bool _shouldSend) public",
            "function enableValidatorViaGateway(address bridgehub,uint256 l1GasPrice,uint256 l2ChainId,uint256 gatewayChainId,address validatorAddress,address gatewayValidatorTimelock, address refundRecipient,bool shouldSend) public",
            "function adminL1L2Tx(address bridgehub,uint256 l1GasPrice,uint256 chainId,address to,uint256 value,bytes calldata data,address refundRecipient,bool _shouldSend) public",
            "function notifyServerMigrationFromGateway(address _bridgehub, uint256 _chainId, bool _shouldSend) public",
            "function notifyServerMigrationToGateway(address _bridgehub, uint256 _chainId, bool _shouldSend) public",
            "function startMigrateChainFromGateway(address bridgehub,uint256 l1GasPrice,uint256 l2ChainId,uint256 gatewayChainId,bytes memory l1DiamondCutData,address refundRecipient,bool _shouldSend)",
            "function prepareUpgradeZKChainOnGateway(uint256 l1GasPrice, uint256 oldProtocolVersion, bytes memory upgradeCutData, address chainDiamondProxyOnGateway, uint256 gatewayChainId, uint256 chainId, address bridgehub, address l1AssetRouterProxy, address refundRecipient, bool shouldSend)",
            "function enableValidator(address bridgehub,uint256 l2ChainId,address validatorAddress,address validatorTimelock,bool _shouldSend) public",
            "function ecosystemAdminExecuteCalls(bytes memory callsToExecute, address ecosystemAdminAddr)"
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

    let calldata = ADMIN_FUNCTIONS
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

    let calldata = ADMIN_FUNCTIONS
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

    let calldata = ADMIN_FUNCTIONS
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
    mode: AdminScriptMode,
    encoded_calls: Vec<u8>,
    forge_args: &ForgeScriptArgs,
    l1_rpc_url: String,
    governance_address: Option<Address>,
) -> anyhow::Result<AdminScriptOutput> {
    // resume doesn't properly work here.
    let mut forge_args = forge_args.clone();
    forge_args.resume = false;

    let governance_address = match governance_address {
        Some(addr) => addr,
        None => {
            let cfg = ecosystem_config
                .get_contracts_config()
                .context("Failed to fetch contracts config to resolve governance address")?;
            cfg.l1.governance_addr
        }
    };

    let calldata = ADMIN_FUNCTIONS
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
        .with_calldata(&calldata);

    let description = "executing governance calls";
    let (forge, spinner_text) = match mode {
        AdminScriptMode::OnlySave => (forge, format!("Preparing calldata for {description}")),
        AdminScriptMode::Broadcast(wallet) => {
            let forge = forge.with_broadcast();
            let forge = fill_forge_private_key(forge, Some(&wallet), WalletOwner::Governor)?;
            check_the_balance(&forge).await?;
            (forge, format!("Executing {description}"))
        }
    };

    let spinner = Spinner::new(&spinner_text);
    forge.run(shell)?;
    spinner.finish();

    let output_path = ACCEPT_GOVERNANCE_SCRIPT_PARAMS.output(&foundry_contracts_path);
    Ok(AdminScriptOutputInner::read(shell, output_path)?.into())
}

#[allow(clippy::too_many_arguments)]
pub async fn ecosystem_admin_execute_calls(
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    ecosystem_admin: &Wallet,
    encoded_calls: Vec<u8>,
    forge_args: &ForgeScriptArgs,
    l1_rpc_url: String,
) -> anyhow::Result<()> {
    // resume doesn't properly work here.
    let mut forge_args = forge_args.clone();
    forge_args.resume = false;

    let ecosystem_admin_addr = ecosystem_config.get_contracts_config()?.l1.chain_admin_addr;

    let calldata = ADMIN_FUNCTIONS
        .encode(
            "ecosystemAdminExecuteCalls",
            (Token::Bytes(encoded_calls), ecosystem_admin_addr),
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
    accept_ownership(shell, ecosystem_admin, forge).await
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

    let calldata = ADMIN_FUNCTIONS
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

    let calldata = ADMIN_FUNCTIONS
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

    let calldata = ADMIN_FUNCTIONS
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

#[derive(Clone)]
pub enum AdminScriptMode {
    OnlySave,
    Broadcast(Wallet),
}

impl AdminScriptMode {
    fn should_send(&self) -> bool {
        matches!(self, AdminScriptMode::Broadcast(_))
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct AdminScriptOutputInner {
    admin_address: Address,
    encoded_data: String,
}

impl ZkStackConfig for AdminScriptOutputInner {}

#[derive(Debug, Clone, Default)]
pub struct AdminScriptOutput {
    pub admin_address: Address,
    pub calls: Vec<AdminCall>,
}

impl From<AdminScriptOutputInner> for AdminScriptOutput {
    fn from(value: AdminScriptOutputInner) -> Self {
        Self {
            admin_address: value.admin_address,
            calls: decode_admin_calls(&hex::decode(value.encoded_data).unwrap()).unwrap(),
        }
    }
}

pub async fn call_script(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    foundry_contracts_path: &Path,
    mode: AdminScriptMode,
    calldata: Bytes,
    l1_rpc_url: String,
    description: &str,
) -> anyhow::Result<AdminScriptOutput> {
    let forge = Forge::new(foundry_contracts_path)
        .script(
            &ACCEPT_GOVERNANCE_SCRIPT_PARAMS.script(),
            forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_calldata(&calldata);

    let (forge, spiner_text) = match mode {
        AdminScriptMode::OnlySave => (forge, format!("Preparing calldata for {description}")),
        AdminScriptMode::Broadcast(wallet) => {
            let forge = forge.with_broadcast();
            let forge = fill_forge_private_key(forge, Some(&wallet), WalletOwner::Governor)?;
            check_the_balance(&forge).await?;

            (forge, format!("Executing {description}"))
        }
    };

    let output_path = ACCEPT_GOVERNANCE_SCRIPT_PARAMS.output(foundry_contracts_path);

    let spinner = Spinner::new(&spiner_text);
    forge.run(shell)?;
    spinner.finish();
    Ok(AdminScriptOutputInner::read(shell, output_path)?.into())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn set_transaction_filterer(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    foundry_contracts_path: &Path,
    mode: AdminScriptMode,
    chain_id: u64,
    bridgehub: Address,
    transaction_filterer_addr: Address,
    l1_rpc_url: String,
) -> anyhow::Result<AdminScriptOutput> {
    let calldata = ADMIN_FUNCTIONS
        .encode(
            "setTransactionFilterer",
            (
                bridgehub,
                U256::from(chain_id),
                transaction_filterer_addr,
                mode.should_send(),
            ),
        )
        .unwrap();

    call_script(
        shell,
        forge_args,
        foundry_contracts_path,
        mode,
        calldata,
        l1_rpc_url,
        &format!(
            "setting transaction filterer {:#?} for chain {}",
            transaction_filterer_addr, chain_id
        ),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn set_da_validator_pair(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    foundry_contracts_path: &Path,
    mode: AdminScriptMode,
    chain_id: u64,
    bridgehub: Address,
    l1_da_validator_address: Address,
    l2_da_validator_address: Address,
    l1_rpc_url: String,
) -> anyhow::Result<AdminScriptOutput> {
    let calldata = ADMIN_FUNCTIONS
        .encode(
            "setDAValidatorPair",
            (
                bridgehub,
                U256::from(chain_id),
                l1_da_validator_address,
                l2_da_validator_address,
                mode.should_send(),
            ),
        )
        .unwrap();

    call_script(
        shell,
        forge_args,
        foundry_contracts_path,
        mode,
        calldata,
        l1_rpc_url,
        &format!(
            "setting data availability validator pair ({:#?}, {:#?}) for chain {}",
            l1_da_validator_address, l2_da_validator_address, chain_id
        ),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn grant_gateway_whitelist(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    foundry_contracts_path: &Path,
    mode: AdminScriptMode,
    chain_id: u64,
    bridgehub: Address,
    grantees: Vec<Address>,
    l1_rpc_url: String,
) -> anyhow::Result<AdminScriptOutput> {
    let comma_separated_grantees = grantees
        .iter()
        .map(|addr| format!("{:#?}", addr))
        .collect::<Vec<_>>()
        .join(", ");
    let calldata = ADMIN_FUNCTIONS
        .encode(
            "grantGatewayWhitelist",
            (
                bridgehub,
                U256::from(chain_id),
                grantees,
                mode.should_send(),
            ),
        )
        .unwrap();

    call_script(
        shell,
        forge_args,
        foundry_contracts_path,
        mode,
        calldata,
        l1_rpc_url,
        &format!("granting gateway whitelist for {comma_separated_grantees}"),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn revoke_gateway_whitelist(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    foundry_contracts_path: &Path,
    mode: AdminScriptMode,
    chain_id: u64,
    bridgehub: Address,
    address: Address,
    l1_rpc_url: String,
) -> anyhow::Result<AdminScriptOutput> {
    let calldata = ADMIN_FUNCTIONS
        .encode(
            "revokeGatewayWhitelist",
            (bridgehub, U256::from(chain_id), address, mode.should_send()),
        )
        .unwrap();

    call_script(
        shell,
        forge_args,
        foundry_contracts_path,
        mode,
        calldata,
        l1_rpc_url,
        &format!("revoking gateway whitelist for {:#?}", address),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn set_da_validator_pair_via_gateway(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    foundry_contracts_path: &Path,
    mode: AdminScriptMode,
    l1_bridgehub: Address,
    max_l1_gas_price: U256,
    l2_chain_id: u64,
    gateway_chain_id: u64,
    l1_da_validator: Address,
    l2_da_validator: Address,
    chain_diamond_proxy_on_gateway: Address,
    refund_recipient: Address,
    l1_rpc_url: String,
) -> anyhow::Result<AdminScriptOutput> {
    let calldata = ADMIN_FUNCTIONS
        .encode(
            "setDAValidatorPairWithGateway",
            (
                l1_bridgehub,
                max_l1_gas_price,
                U256::from(l2_chain_id),
                U256::from(gateway_chain_id),
                l1_da_validator,
                l2_da_validator,
                chain_diamond_proxy_on_gateway,
                refund_recipient,
                mode.should_send(),
            ),
        )
        .unwrap();

    call_script(
        shell,
        forge_args,
        foundry_contracts_path,
        mode,
        calldata,
        l1_rpc_url,
        &format!(
            "setting DA validator pair (SL = {:#?}, L2 = {:#?}) via gateway",
            l1_da_validator, l2_da_validator
        ),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn enable_validator_via_gateway(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    foundry_contracts_path: &Path,
    mode: AdminScriptMode,
    l1_bridgehub: Address,
    max_l1_gas_price: U256,
    l2_chain_id: u64,
    gateway_chain_id: u64,
    validator_address: Address,
    gateway_validator_timelock: Address,
    refund_recipient: Address,
    l1_rpc_url: String,
) -> anyhow::Result<AdminScriptOutput> {
    let calldata = ADMIN_FUNCTIONS
        .encode(
            "enableValidatorViaGateway",
            (
                l1_bridgehub,
                max_l1_gas_price,
                U256::from(l2_chain_id),
                U256::from(gateway_chain_id),
                validator_address,
                gateway_validator_timelock,
                refund_recipient,
                mode.should_send(),
            ),
        )
        .unwrap();

    call_script(
        shell,
        forge_args,
        foundry_contracts_path,
        mode,
        calldata,
        l1_rpc_url,
        &format!("enabling validator {:#?} via gateway", validator_address),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn enable_validator(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    foundry_contracts_path: &Path,
    mode: AdminScriptMode,
    l1_bridgehub: Address,
    l2_chain_id: u64,
    validator_address: Address,
    validator_timelock: Address,
    l1_rpc_url: String,
) -> anyhow::Result<AdminScriptOutput> {
    let calldata = ADMIN_FUNCTIONS
        .encode(
            "enableValidator",
            (
                l1_bridgehub,
                U256::from(l2_chain_id),
                validator_address,
                validator_timelock,
                mode.should_send(),
            ),
        )
        .unwrap();

    call_script(
        shell,
        forge_args,
        foundry_contracts_path,
        mode,
        calldata,
        l1_rpc_url,
        &format!("enabling validator {:#?} via gateway", validator_address),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn notify_server_migration_to_gateway(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    foundry_contracts_path: &Path,
    mode: AdminScriptMode,
    chain_id: u64,
    bridgehub: Address,
    l1_rpc_url: String,
) -> anyhow::Result<AdminScriptOutput> {
    let calldata = ADMIN_FUNCTIONS
        .encode(
            "notifyServerMigrationToGateway",
            (bridgehub, U256::from(chain_id), mode.should_send()),
        )
        .unwrap();

    call_script(
        shell,
        forge_args,
        foundry_contracts_path,
        mode,
        calldata,
        l1_rpc_url,
        "notifying migration to gateway to the server",
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn finalize_migrate_to_gateway(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    foundry_contracts_path: &Path,
    mode: AdminScriptMode,
    bridgehub: Address,
    l1_gas_price: u64,
    l2_chain_id: u64,
    gateway_chain_id: u64,
    gateway_diamond_cut_data: Bytes,
    refund_recipient: Address,
    l1_rpc_url: String,
) -> anyhow::Result<AdminScriptOutput> {
    let calldata = ADMIN_FUNCTIONS
        .encode(
            "migrateChainToGateway",
            (
                bridgehub,
                U256::from(l1_gas_price),
                U256::from(l2_chain_id),
                U256::from(gateway_chain_id),
                gateway_diamond_cut_data,
                refund_recipient,
                mode.should_send(),
            ),
        )
        .unwrap();

    call_script(
        shell,
        forge_args,
        foundry_contracts_path,
        mode,
        calldata,
        l1_rpc_url,
        "finalizing migration to gateway",
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn notify_server_migration_from_gateway(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    foundry_contracts_path: &Path,
    mode: AdminScriptMode,
    chain_id: u64,
    bridgehub: Address,
    l1_rpc_url: String,
) -> anyhow::Result<AdminScriptOutput> {
    let calldata = ADMIN_FUNCTIONS
        .encode(
            "notifyServerMigrationFromGateway",
            (bridgehub, U256::from(chain_id), mode.should_send()),
        )
        .unwrap();

    call_script(
        shell,
        forge_args,
        foundry_contracts_path,
        mode,
        calldata,
        l1_rpc_url,
        "notifying migration from gateway to the server",
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn admin_l1_l2_tx(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    foundry_contracts_path: &Path,
    mode: AdminScriptMode,
    bridgehub: Address,
    l1_gas_price: u64,
    chain_id: u64,
    to: Address,
    value: U256,
    data: Bytes,
    refund_recipient: Address,
    l1_rpc_url: String,
) -> anyhow::Result<AdminScriptOutput> {
    let hex_encoded_data = hex::encode(&data.0);
    let calldata = ADMIN_FUNCTIONS
        .encode(
            "adminL1L2Tx",
            (
                bridgehub,
                U256::from(l1_gas_price),
                U256::from(chain_id),
                to,
                value,
                data,
                refund_recipient,
                mode.should_send(),
            ),
        )
        .unwrap();

    call_script(
        shell,
        forge_args,
        foundry_contracts_path,
        mode,
        calldata,
        l1_rpc_url,
        &format!(
            "executing ChainAdmin transaction (to = {:#?}, data = {}, value = {:#?})",
            to, hex_encoded_data, value,
        ),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn prepare_upgrade_zk_chain_on_gateway(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    foundry_contracts_path: &Path,
    mode: AdminScriptMode,
    chain_id: u64,
    gateway_chain_id: u64,
    bridgehub: Address,
    l1_gas_price: u64,
    old_protocol_version: u64,
    chain_diamond_proxy_on_gateway: Address,
    l1_asset_router_proxy: Address,
    refund_recipient: Address,
    upgrade_cut_data: Bytes,
    l1_rpc_url: String,
) -> anyhow::Result<AdminScriptOutput> {
    let calldata = ADMIN_FUNCTIONS
        .encode(
            "prepareUpgradeZKChainOnGateway",
            (
                U256::from(l1_gas_price),
                U256::from(old_protocol_version),
                upgrade_cut_data,
                chain_diamond_proxy_on_gateway,
                U256::from(gateway_chain_id),
                U256::from(chain_id),
                bridgehub,
                l1_asset_router_proxy,
                refund_recipient,
                mode.should_send(),
            ),
        )
        .unwrap();

    call_script(
        shell,
        forge_args,
        foundry_contracts_path,
        mode,
        calldata,
        l1_rpc_url,
        "prepare calldata to upgrade ZK chain on Gateway",
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn start_migrate_chain_from_gateway(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    foundry_contracts_path: &Path,
    mode: AdminScriptMode,
    bridgehub: Address,
    l1_gas_price: u64,
    l2_chain_id: u64,
    gateway_chain_id: u64,
    l1_diamond_cut_data: Bytes,
    refund_recipient: Address,
    l1_rpc_url: String,
) -> anyhow::Result<AdminScriptOutput> {
    let calldata = ADMIN_FUNCTIONS
        .encode(
            "startMigrateChainFromGateway",
            (
                bridgehub,
                U256::from(l1_gas_price),
                U256::from(l2_chain_id),
                U256::from(gateway_chain_id),
                l1_diamond_cut_data,
                refund_recipient,
                mode.should_send(),
            ),
        )
        .unwrap();

    call_script(
        shell,
        forge_args,
        foundry_contracts_path,
        mode,
        calldata,
        l1_rpc_url,
        "starting chain migration from gateway",
    )
    .await
}
