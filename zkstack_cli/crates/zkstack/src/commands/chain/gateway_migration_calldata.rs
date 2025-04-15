// src/commands/chain/migrate_to_gateway_calldata.rs

use std::{path::Path, sync::Arc};

use anyhow::Context;
use clap::Parser;
use ethers::{
    abi::{encode, Token},
    contract::{abigen, BaseContract},
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{Bytes, Filter, TransactionReceipt, TransactionRequest},
    utils::{hex, keccak256},
};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    logger,
    wallets::Wallet,
};
use zkstack_cli_config::{
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, EcosystemConfig,
};
use zkstack_cli_types::L1BatchCommitmentMode;
use zksync_basic_types::{Address, H256, U256, U64};
use zksync_config::configs::gateway::{GatewayChainConfig, GatewayConfig};
use zksync_contracts::chain_admin_contract;
use zksync_system_constants::L2_BRIDGEHUB_ADDRESS;
use zksync_types::{
    address_to_u256, h256_to_u256, server_notification::GatewayMigrationNotification,
    u256_to_address, u256_to_h256, web3::ValueOrArray,
};
use zksync_web3_decl::namespaces::UnstableNamespaceClient;

use super::{
    admin_call_builder::{self, AdminCall, AdminCallBuilder},
    gateway_migration::MigrationDirection,
    notify_server_calldata::{get_notify_server_calls, NotifyServerCalldataArgs},
    utils::{display_admin_script_output, get_default_foundry_path, get_zk_client},
};
use crate::{
    accept_ownership::{
        admin_l1_l2_tx, enable_validator_via_gateway, finalize_migrate_to_gateway,
        notify_server_migration_from_gateway, notify_server_migration_to_gateway,
        set_da_validator_pair_via_gateway, AdminScriptOutput,
    },
    commands::chain::utils::get_ethers_provider,
    messages::MSG_CHAIN_NOT_INITIALIZED,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct MigrateToGatewayArgs {
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,

    #[clap(long)]
    pub gateway_chain_name: String,
}

// 50 gwei
const MAX_EXPECTED_L1_GAS_PRICE: u64 = 50_000_000_000;

abigen!(
    BridgehubAbi,
    r"[
    function settlementLayer(uint256)(uint256)
    function getZKChain(uint256)(address)
    function ctmAssetIdToAddress(bytes32)(address)
    function ctmAssetIdFromChainId(uint256)(bytes32)
    function baseTokenAssetId(uint256)(bytes32)
    function chainTypeManager(uint256)(address)
]"
);

abigen!(
    ZkChainAbi,
    r"[
    function getDAValidatorPair()(address,address)
    function getAdmin()(address)
    function getProtocolVersion()(uint256)
]"
);

abigen!(
    ChainTypeManagerAbi,
    r"[
    function validatorTimelock()(address)
    function forwardedBridgeMint(uint256 _chainId,bytes calldata _ctmData)(address)
    function serverNotifierAddress()(address)
]"
);

abigen!(
    ValidatorTimelockAbi,
    r"[
    function validators(uint256 _chainId, address _validator)(bool)
]"
);

/// Each migration to or from ZK gateway has multiple states it can be in.
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum GatewayMigrationState {
    /// The state that represents that the migration has not yet started.
    NotStarted,
    /// The chain admin has sent the notification
    NotificationSent,
    /// The server has received the notification, but it is not yet ready for the migration.
    NotificationReceived,
    /// The server has received the notification and has no pending transactions
    ServerReady,
    /// The server is ready and the migration has started, but the server has not started sending transactions
    /// to the new settlement layer yet.
    AwaitingFinalization,
    /// The migration has finished.
    Finished,
}

const IS_PERMANENT_ROLLUP_SLOT: u64 = 57;

fn apply_l1_to_l2_alias(addr: Address) -> Address {
    let offset: Address = "1111000000000000000000000000000000001111".parse().unwrap();
    let addr_with_offset = address_to_u256(&addr) + address_to_u256(&offset);

    u256_to_address(&addr_with_offset)
}

// The most reliable way to precompute the address is to simulate `createNewChain` function
async fn precompute_chain_address_on_gateway(
    l2_chain_id: u64,
    base_token_asset_id: H256,
    new_l2_admin: Address,
    protocol_version: U256,
    gateway_diamond_cut: Vec<u8>,
    gw_ctm: ChainTypeManagerAbi<Provider<ethers::providers::Http>>,
) -> anyhow::Result<Address> {
    let ctm_data = encode(&[
        Token::FixedBytes(base_token_asset_id.0.into()),
        Token::Address(new_l2_admin),
        Token::Uint(protocol_version),
        Token::Bytes(gateway_diamond_cut),
    ]);

    let result = gw_ctm
        .forwarded_bridge_mint(l2_chain_id.into(), ctm_data.into())
        .from(L2_BRIDGEHUB_ADDRESS)
        .await?;

    Ok(result)
}

const EVENTS_BLOCK_RANGE: u64 = 50_000;

async fn get_latest_notification_event_from_l1(
    l2_chain_id: u64,
    l1_ctm: ChainTypeManagerAbi<Provider<Http>>,
    l1_provider: Arc<Provider<Http>>,
) -> anyhow::Result<Option<GatewayMigrationNotification>> {
    logger::info("Searching for the latest migration notifications...");
    let server_notifier_address = l1_ctm.server_notifier_address().await?;

    // Get the latest block so we know how far we can go
    let latest_block = l1_provider
        .get_block_number()
        .await
        .expect("Failed to fetch latest block")
        .as_u64();

    let filter = Filter::new()
        .address(server_notifier_address)
        .topic0(ethers::types::ValueOrArray::Array(
            GatewayMigrationNotification::get_server_notifier_topics(),
        ))
        .from_block(latest_block.saturating_sub(EVENTS_BLOCK_RANGE))
        .topic1(u256_to_h256(U256::from(l2_chain_id)))
        .to_block(latest_block);

    let mut result_logs = l1_provider.get_logs(&filter).await?;

    if result_logs.is_empty() {
        return Ok(None);
    }
    let latest_log = result_logs.pop().unwrap();

    let result = GatewayMigrationNotification::from_topic(latest_log.topics[0])
        .expect("Failed to parse event");

    match result {
        GatewayMigrationNotification::FromGateway => {
            logger::info(format!(
                "Latest event is MigrationDirection::FromGateway at tx {:#?}",
                latest_log.transaction_hash
            ));
        }
        GatewayMigrationNotification::ToGateway => {
            logger::info(format!(
                "Latest event is MigrationDirection::ToGateway at tx {:#?}",
                latest_log.transaction_hash
            ));
        }
    }

    Ok(Some(result))
}

pub(crate) async fn get_gateway_migration_state(
    l1_rpc_url: String,
    l1_bridgehub_addr: Address,
    l2_chain_id: u64,
    l2_rpc_url: String,
    direction: MigrationDirection,
) -> anyhow::Result<GatewayMigrationState> {
    let l1_provider = get_ethers_provider(&l1_rpc_url)?;
    let l1_chain_id = l1_provider.get_chainid().await?;
    let l1_bridgehub = BridgehubAbi::new(l1_bridgehub_addr, l1_provider.clone());

    let l1_ctm_address = l1_bridgehub.chain_type_manager(l2_chain_id.into()).await?;
    let l1_ctm = ChainTypeManagerAbi::new(l1_ctm_address, l1_provider.clone());

    if matches!(direction, MigrationDirection::FromGateway) {
        // TODO(X): add script support for migrating away from Gateway
        anyhow::bail!("Currently the scripts only support for migrating on top of Gateway");
    }

    let current_sl = l1_bridgehub.settlement_layer(l2_chain_id.into()).await?;

    let zk_client = get_zk_client(&l2_rpc_url, l2_chain_id)?;

    let gateway_migration_status = match zk_client.gateway_migration_status().await {
        Ok(status) => status,
        Err(e) => {
            anyhow::bail!(format!("Failed to retrieve gateway migration status from the server. Error: {:#?} Ensure that the server supports this method and has `unstable` namespace turned on", e));
        }
    };

    if current_sl != l1_chain_id {
        // The chain now has a new settlement layer registered on L1, but the server may
        // not yet use it.
        if gateway_migration_status.settlement_layer.is_gateway() {
            return Ok(GatewayMigrationState::Finished);
        } else {
            return Ok(GatewayMigrationState::AwaitingFinalization);
        }
    }

    let Some(latest_event) =
        get_latest_notification_event_from_l1(l2_chain_id, l1_ctm, l1_provider).await?
    else {
        // All migrations should start with a notification
        return Ok(GatewayMigrationState::NotStarted);
    };

    let expected_notification = direction.expected_notificaation();

    if latest_event != expected_notification {
        // It is likely a leftover from a previous migration
        return Ok(GatewayMigrationState::NotStarted);
    }

    // At this point we know that at least the event to notify the server has been sent, now we need to check
    // whether the server has received the event.
    if gateway_migration_status.latest_event != Some(expected_notification) {
        // The server has not yet seen the event, so the notification is only sent, but not received
        return Ok(GatewayMigrationState::NotificationSent);
    }

    // At this point we know that the notification has been received, we need to check whether the server has processed the corresponding events.

    let unconfirmed_txs = zk_client.get_unconfirmed_txs_count().await?;

    if unconfirmed_txs != 0 {
        // The server has received the notification, but there are still some pending txs
        return Ok(GatewayMigrationState::NotificationReceived);
    }

    // We know that the server is ready, but the new settlement layer has not yet been reflected
    Ok(GatewayMigrationState::ServerReady)
}

#[derive(Parser, Debug)]
pub(crate) struct MigrateToGatewayParams {
    pub(crate) l1_rpc_url: String,
    pub(crate) l1_bridgehub_addr: Address,
    pub(crate) max_l1_gas_price: u64,
    pub(crate) l2_chain_id: u64,
    pub(crate) gateway_chain_id: u64,
    pub(crate) gateway_diamond_cut: Vec<u8>,
    pub(crate) gateway_rpc_url: String,
    pub(crate) new_sl_da_validator: Address,
    pub(crate) validator_1: Address,
    pub(crate) validator_2: Address,
    pub(crate) min_validator_balance: U256,
    pub(crate) refund_recipient: Option<Address>,
}

pub(crate) async fn get_migrate_to_gateway_calls(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    foundry_contracts_path: &Path,
    params: MigrateToGatewayParams,
) -> anyhow::Result<(Address, Vec<AdminCall>)> {
    // TODO: add checks about chain notification.

    let refund_recipient = params.refund_recipient.unwrap_or(params.validator_1);
    let mut result = vec![];

    let l1_provider = get_ethers_provider(&params.l1_rpc_url)?;
    let gw_provider = get_ethers_provider(&params.gateway_rpc_url)?;

    let l1_bridgehub = BridgehubAbi::new(params.l1_bridgehub_addr, l1_provider.clone());
    let gw_bridgehub = BridgehubAbi::new(L2_BRIDGEHUB_ADDRESS, gw_provider.clone());

    let current_settlement_layer = l1_bridgehub
        .settlement_layer(params.l2_chain_id.into())
        .await?;

    println!("here");

    let zk_chain_l1_address = l1_bridgehub.get_zk_chain(params.l2_chain_id.into()).await?;

    if zk_chain_l1_address == Address::zero() {
        anyhow::bail!("Chain with id {} does not exist!", params.l2_chain_id);
    }

    println!("here2");

    // Checking whether the user has already done the migration
    if current_settlement_layer == U256::from(params.gateway_chain_id) {
        // TODO: it may happen that the user has started the migration, but it failed for some reason (e.g. the provided
        // diamond cut was not correct).
        // The recovery of the chain is not handled by the tool right now.
        anyhow::bail!("The chain is already on top of Gateway!");
    }
    println!("here4");

    let ctm_asset_id = l1_bridgehub
        .ctm_asset_id_from_chain_id(params.l2_chain_id.into())
        .await?;
    let ctm_gw_address = gw_bridgehub.ctm_asset_id_to_address(ctm_asset_id).await?;
    println!("here3");

    if ctm_gw_address == Address::zero() {
        anyhow::bail!("{} does not have a CTM deployed!", params.gateway_chain_id);
    }

    let gw_ctm = ChainTypeManagerAbi::new(ctm_gw_address, gw_provider.clone());
    let gw_validator_timelock_addr = gw_ctm.validator_timelock().await?;
    let gw_validator_timelock =
        ValidatorTimelockAbi::new(gw_validator_timelock_addr, gw_provider.clone());

    let l1_zk_chain = ZkChainAbi::new(zk_chain_l1_address, l1_provider.clone());
    let chain_admin_address = l1_zk_chain.get_admin().await?;
    let zk_chain_gw_address = {
        let recorded_zk_chain_gw_address =
            gw_bridgehub.get_zk_chain(params.l2_chain_id.into()).await?;
        if recorded_zk_chain_gw_address == Address::zero() {
            let expected_address = precompute_chain_address_on_gateway(
                params.l2_chain_id,
                H256(
                    l1_bridgehub
                        .base_token_asset_id(params.l2_chain_id.into())
                        .await?,
                ),
                apply_l1_to_l2_alias(l1_zk_chain.get_admin().await?),
                l1_zk_chain.get_protocol_version().await?,
                params.gateway_diamond_cut.clone(),
                gw_ctm,
            )
            .await?;

            expected_address
        } else {
            recorded_zk_chain_gw_address
        }
    };

    println!("here6");

    let finalize_migrate_to_gateway_output = finalize_migrate_to_gateway(
        shell,
        forge_args,
        foundry_contracts_path,
        crate::accept_ownership::AdminScriptMode::OnlySave,
        params.l1_bridgehub_addr,
        params.max_l1_gas_price,
        params.l2_chain_id,
        params.gateway_chain_id,
        params.gateway_diamond_cut.into(),
        refund_recipient,
        params.l1_rpc_url.clone(),
    )
    .await?;

    result.extend(finalize_migrate_to_gateway_output.calls);

    // Changing L2 DA validator while migrating to gateway is not recommended; we allow changing only the SL one
    let (_, l2_da_validator) = l1_zk_chain.get_da_validator_pair().await?;

    // Unfortunately, there is no getter for whether a chain is a permanent rollup, we have to
    // read storage here.
    let is_permanent_rollup_slot = l1_provider
        .get_storage_at(zk_chain_l1_address, H256::from_low_u64_be(57), None)
        .await?;
    if is_permanent_rollup_slot == H256::from_low_u64_be(1) {
        // TODO(X): We should really check it on our own here, but it is hard with the current interfaces
        println!("WARNING: Your chain is a permanent rollup! Ensure that the new L1 SL provider is compatible with Gateway RollupDAManager!");
    }

    let da_validator_encoding_result = set_da_validator_pair_via_gateway(
        shell,
        forge_args,
        foundry_contracts_path,
        crate::accept_ownership::AdminScriptMode::OnlySave,
        params.l1_bridgehub_addr,
        params.max_l1_gas_price.into(),
        params.l2_chain_id,
        params.gateway_chain_id,
        params.new_sl_da_validator,
        l2_da_validator,
        zk_chain_gw_address,
        refund_recipient,
        params.l1_rpc_url.clone(),
    )
    .await?;

    result.extend(da_validator_encoding_result.calls);

    // 4. If validators are not yet present, please include.
    for validator in [params.validator_1, params.validator_2] {
        if !gw_validator_timelock
            .validators(params.l2_chain_id.into(), validator)
            .await?
        {
            let enable_validator_calls = enable_validator_via_gateway(
                shell,
                forge_args,
                foundry_contracts_path,
                crate::accept_ownership::AdminScriptMode::OnlySave,
                params.l1_bridgehub_addr,
                params.max_l1_gas_price.into(),
                params.l2_chain_id,
                params.gateway_chain_id,
                validator,
                gw_validator_timelock_addr,
                refund_recipient,
                params.l1_rpc_url.clone(),
            )
            .await?;
            result.extend(enable_validator_calls.calls);
        }

        let current_validator_balance = gw_provider.get_balance(validator, None).await?;
        println!("current balance = {}", current_validator_balance);
        if current_validator_balance < params.min_validator_balance {
            println!(
                "Sohuld send {}",
                params.min_validator_balance - current_validator_balance
            );
            let supply_validator_balance_calls = admin_l1_l2_tx(
                shell,
                forge_args,
                foundry_contracts_path,
                crate::accept_ownership::AdminScriptMode::OnlySave,
                params.l1_bridgehub_addr,
                params.max_l1_gas_price.into(),
                params.gateway_chain_id,
                validator,
                params.min_validator_balance - current_validator_balance,
                Default::default(),
                refund_recipient,
                params.l1_rpc_url.clone(),
            )
            .await?;
            result.extend(supply_validator_balance_calls.calls);
        }
    }

    Ok((chain_admin_address, result))
}

#[derive(Parser, Debug)]
#[command()]
pub(crate) struct MigrateToGatewayCalldataScriptArgs {
    pub l1_rpc_url: String,
    pub l1_bridgehub_addr: Address,
    pub max_l1_gas_price: u64,
    pub l2_chain_id: u64,
    pub gateway_chain_id: u64,

    pub gateway_config_path: String,

    pub gateway_rpc_url: String,
    pub new_sl_da_validator: Address,
    pub validator_1: Address,
    pub validator_2: Address,
    pub min_validator_balance: U256,
    pub refund_recipient: Option<Address>,

    /// RPC URL of the chain being migrated (L2).
    pub l2_rpc_url: Option<String>,

    /// Whether to force providing the full migration calldata even if the chain
    /// isn't strictly ready for final calls.
    pub no_cross_check: Option<bool>,
}

impl MigrateToGatewayCalldataScriptArgs {
    /// Converts into `MigrateToGatewayParams` by injecting the provided diamond cut.
    pub fn into_migrate_params(self, gateway_diamond_cut: Vec<u8>) -> MigrateToGatewayParams {
        MigrateToGatewayParams {
            l1_rpc_url: self.l1_rpc_url,
            l1_bridgehub_addr: self.l1_bridgehub_addr,
            max_l1_gas_price: self.max_l1_gas_price,
            l2_chain_id: self.l2_chain_id,
            gateway_chain_id: self.gateway_chain_id,
            gateway_diamond_cut,
            gateway_rpc_url: self.gateway_rpc_url,
            new_sl_da_validator: self.new_sl_da_validator,
            validator_1: self.validator_1,
            validator_2: self.validator_2,
            min_validator_balance: self.min_validator_balance,
            refund_recipient: self.refund_recipient,
        }
    }
}

/// Produces the calldata necessary to perform (or continue) a migration to Gateway.
///
pub async fn run(shell: &Shell, params: MigrateToGatewayCalldataScriptArgs) -> anyhow::Result<()> {
    let forge_args = Default::default();
    let contracts_foundry_path = get_default_foundry_path()?;

    let should_cross_check = !params.no_cross_check.unwrap_or_default();

    if should_cross_check {
        let state = get_gateway_migration_state(
            params.l1_rpc_url.clone(),
            params.l1_bridgehub_addr,
            params.l2_chain_id,
            params
                .l2_rpc_url
                .clone()
                .context("L2 RPC URL must be provided for cross checking")?,
            MigrationDirection::ToGateway,
        )
        .await?;

        match state {
            GatewayMigrationState::NotStarted => {
                logger::info("Notification has not yet been sent. Please use the command to send notification");
                return Ok(());
            }
            GatewayMigrationState::NotificationSent => {
                logger::info("Notification has been sent, but the server has not yet picked it up. Please wait");
                return Ok(());
            }
            GatewayMigrationState::NotificationReceived => {
                logger::info("The server has received the notification about the migration, but it needs to finish all outstanding transactions. Please wait");
                return Ok(());
            }
            GatewayMigrationState::ServerReady => {
                logger::info(
                    "The server is ready to start the migration. Preparing the calldata...",
                );
                // It is the expected case, it will be handled later in the file
            }
            GatewayMigrationState::AwaitingFinalization => {
                logger::info("The transaction to migrate chain on top of Gateway has been submitted, but the server has not yet processed it");
                return Ok(());
            }
            GatewayMigrationState::Finished => {
                logger::info("The migration in this direction has been already finished");
                return Ok(());
            }
        }
    }

    let gateway_config = GatewayConfig::read(shell, &params.gateway_config_path)
        .context("Failed to read the gateway config path")?;

    let (admin_address, calls) = get_migrate_to_gateway_calls(
        &shell,
        &forge_args,
        &contracts_foundry_path,
        params.into_migrate_params(gateway_config.diamond_cut_data.0),
    )
    .await?;

    display_admin_script_output(AdminScriptOutput {
        admin_address,
        calls,
    });

    Ok(())
}
