use std::{path::Path, sync::Arc};

use anyhow::Context;
use clap::Parser;
use ethers::{
    abi::{encode, parse_abi, ParamType, Token},
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
use zksync_config::configs::gateway::GatewayChainConfig;
use zksync_contracts::chain_admin_contract;
use zksync_system_constants::L2_BRIDGEHUB_ADDRESS;
use zksync_types::{
    address_to_u256, h256_to_u256, server_notification::GatewayMigrationNotification,
    u256_to_address, u256_to_h256, web3::ValueOrArray,
};
use zksync_web3_decl::namespaces::UnstableNamespaceClient;

use super::{
    admin_call_builder::{self, AdminCall, AdminCallBuilder},
    notify_server_calldata::{get_notify_server_calls, NotifyServerCalldataArgs},
    utils::get_zk_client,
};
use crate::{
    accept_ownership::{
        admin_l1_l2_tx, enable_validator_via_gateway, finalize_migrate_to_gateway,
        notify_server_migration_from_gateway, notify_server_migration_to_gateway,
        set_da_validator_pair_via_gateway,
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
enum GatewayMigrationState {
    /// The state that represents that the migration has not yet started.
    NotStarted,
    /// The chain admin has sent the notification
    NotificationSent,
    /// The server has received the notification
    NotificationReceived,
    /// The server has received the notification and has no pending transactions
    ServerReady,
    /// The server is ready and the migration has started, but the server has not started sending transactions
    /// to the new settlement layer yet.
    AwaitingFinalization,
    /// (Only for migration from Gateway): the migration has been complete on the GW, but needs to be finalized on L1
    PendingL1Finalization,
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

struct ServerGatewayMigrationStatus {
    // status:
    unconfirmed_txs: usize,
}

async fn get_gateway_migration_state(
    l1_provider: Arc<Provider<Http>>,
    l1_bridgehub: BridgehubAbi<Provider<Http>>,
    l1_ctm: ChainTypeManagerAbi<Provider<Http>>,
    l2_chain_id: u64,
    l2_rpc_url: String,
    gateway_chain_id: u64,
    direction: MigrationDirection,
) -> anyhow::Result<GatewayMigrationState> {
    if matches!(direction, MigrationDirection::FromGateway) {
        // TODO(X): add script support for migrating away from Gateway
        anyhow::bail!("Currently the scripts only support for migrating on top of Gateway");
    }

    let current_sl = l1_bridgehub.settlement_layer(l2_chain_id.into()).await?;

    let zk_client = get_zk_client(&l2_rpc_url, l2_chain_id)?;

    let gateway_migration_status = zk_client.gateway_migration_status().await?;

    if current_sl == U256::from(gateway_chain_id) {
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

pub(crate) struct MigrateToGatewayParams {
    l1_rpc_url: String,
    l1_bridgehub_addr: Address,
    max_l1_gas_price: u64,
    l2_chain_id: u64,
    gateway_chain_id: u64,
    gateway_diamond_cut: Vec<u8>,
    gateway_rpc_url: String,
    new_sl_da_validator: Address,
    validator_1: Address,
    validator_2: Address,
    min_validator_balance: U256,
    refund_recipient: Option<Address>,
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

pub async fn run(args: MigrateToGatewayArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let chain_name = global_config().chain_name.clone();
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    let gateway_chain_config = ecosystem_config
        .load_chain(Some(args.gateway_chain_name.clone()))
        .context("Gateway not present")?;
    let gateway_chain_id = gateway_chain_config.chain_id.as_u64();
    let gateway_gateway_config = gateway_chain_config
        .get_gateway_config()
        .context("Gateway config not present")?;

    let l1_url = chain_config
        .get_secrets_config()
        .await?
        .get::<String>("l1.l1_rpc_url")?;

    let genesis_config = chain_config.get_genesis_config().await?;
    let gateway_contract_config = gateway_chain_config.get_contracts_config()?;

    let chain_contracts_config = chain_config.get_contracts_config().unwrap();
    let chain_access_control_restriction = chain_contracts_config
        .l1
        .access_control_restriction_addr
        .context("chain_access_control_restriction")?;

    logger::info("Migrating the chain to the Gateway...");

    let general_config = gateway_chain_config.get_general_config().await?;
    let gw_rpc_url = general_config.get::<String>("api.web3_json_rpc.http_url")?;

    let is_rollup = matches!(
        genesis_config.get("l1_batch_commit_data_generator_mode")?,
        L1BatchCommitmentMode::Rollup
    );

    let gateway_da_validator_address = if is_rollup {
        gateway_gateway_config.relayed_sl_da_validator
    } else {
        gateway_gateway_config.validium_da_validator
    };
    let chain_secrets_config = chain_config.get_wallets_config().unwrap();

    let (chain_admin, calls) = get_migrate_to_gateway_calls(
        shell,
        &args.forge_args,
        &chain_config.path_to_l1_foundry(),
        MigrateToGatewayParams {
            l1_rpc_url: l1_url.clone(),
            l1_bridgehub_addr: chain_contracts_config
                .ecosystem_contracts
                .bridgehub_proxy_addr,
            max_l1_gas_price: MAX_EXPECTED_L1_GAS_PRICE,
            l2_chain_id: chain_config.chain_id.as_u64(),
            gateway_chain_id: gateway_chain_config.chain_id.as_u64(),
            gateway_diamond_cut: gateway_gateway_config.diamond_cut_data.0.clone().into(),
            gateway_rpc_url: gw_rpc_url.clone(),
            new_sl_da_validator: gateway_da_validator_address,
            validator_1: chain_secrets_config.blob_operator.address,
            validator_2: chain_secrets_config.operator.address,
            min_validator_balance: U256::from(10).pow(21.into()).into(),
            refund_recipient: None,
        },
    )
    .await?;

    if calls.is_empty() {
        logger::info("Chain already migrated!");
        return Ok(());
    }

    let (calldata, value) = AdminCallBuilder::new(calls).compile_full_calldata();

    let receipt = send_tx(
        chain_admin,
        calldata,
        value,
        l1_url.clone(),
        chain_config
            .get_wallets_config()?
            .governor
            .private_key_h256()
            .unwrap(),
    )
    .await?;

    let gateway_provider = get_ethers_provider(&gw_rpc_url)?;
    extract_and_wait_for_priority_ops(
        receipt,
        gateway_contract_config.l1.diamond_proxy_addr,
        gateway_provider.clone(),
    )
    .await?;

    let mut chain_secrets_config = chain_config.get_secrets_config().await?.patched();
    chain_secrets_config.insert("l1.gateway_rpc_url", gw_rpc_url)?;
    chain_secrets_config.save().await?;

    let gw_bridgehub = BridgehubAbi::new(L2_BRIDGEHUB_ADDRESS, gateway_provider);

    let gateway_chain_config = GatewayChainConfig::from_gateway_and_chain_data(
        &gateway_gateway_config,
        gw_bridgehub
            .get_zk_chain(chain_config.chain_id.as_u64().into())
            .await?,
        // FIXME: no chain admin is supported here
        Address::zero(),
        gateway_chain_id.into(),
    );
    gateway_chain_config.save_with_base_path(shell, chain_config.configs.clone())?;

    Ok(())
}

pub(crate) async fn await_for_tx_to_complete(
    gateway_provider: &Arc<Provider<Http>>,
    hash: H256,
) -> anyhow::Result<()> {
    logger::info(&format!(
        "Waiting for transaction with hash {:#?} to complete...",
        hash
    ));
    while gateway_provider
        .get_transaction_receipt(hash)
        .await?
        .is_none()
    {
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    // We do not handle network errors
    let receipt = gateway_provider
        .get_transaction_receipt(hash)
        .await?
        .unwrap();

    if receipt.status == Some(U64::from(1)) {
        logger::info("Transaction completed successfully!");
    } else {
        panic!("Transaction failed! Receipt: {:?}", receipt);
    }

    Ok(())
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum MigrationDirection {
    FromGateway,
    ToGateway,
}

impl MigrationDirection {
    fn expected_notificaation(self) -> GatewayMigrationNotification {
        match self {
            Self::FromGateway => GatewayMigrationNotification::FromGateway,
            Self::ToGateway => GatewayMigrationNotification::ToGateway,
        }
    }
}

pub(crate) async fn notify_server(
    args: ForgeScriptArgs,
    shell: &Shell,
    direction: MigrationDirection,
) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    let l1_url = chain_config
        .get_secrets_config()
        .await?
        .get::<String>("l1.l1_rpc_url")?;

    let calls = get_notify_server_calls(
        shell,
        args,
        &chain_config.path_to_l1_foundry(),
        NotifyServerCalldataArgs {
            bridgehub_addr: ecosystem_config
                .get_contracts_config()?
                .ecosystem_contracts
                .bridgehub_proxy_addr,
            l2_chain_id: chain_config.chain_id.as_u64(),
            l1_rpc_url: l1_url.clone(),
        },
        direction,
    )
    .await?;

    let (data, value) = AdminCallBuilder::new(calls.calls).compile_full_calldata();

    send_tx(
        calls.admin_address,
        data,
        value,
        l1_url,
        chain_config
            .get_wallets_config()?
            .governor
            .private_key_h256()
            .unwrap(),
    )
    .await?;

    Ok(())
}

pub(crate) async fn send_tx(
    to: Address,
    data: Vec<u8>,
    value: U256,
    l1_rpc_url: String,
    private_key: H256,
) -> anyhow::Result<TransactionReceipt> {
    // 1. Connect to provider
    let provider = Provider::<Http>::try_from(&l1_rpc_url)?;

    // 2. Set up wallet (signer)
    let wallet: LocalWallet = LocalWallet::from_bytes(private_key.as_bytes())?;
    let wallet = wallet.with_chain_id(provider.get_chainid().await?.as_u64()); // Mainnet

    // 3. Create a transaction
    let tx = TransactionRequest::new().to(to).data(data).value(value);

    // 4. Sign the transaction
    let client = SignerMiddleware::new(provider.clone(), wallet.clone());
    let pending_tx = client.send_transaction(tx, None).await?;

    println!(
        "Transaction {:#?} has been sent! Waiting...",
        pending_tx.tx_hash()
    );

    // 5. Await receipt
    let receipt: TransactionReceipt = pending_tx.await?.context("Receipt not found")?;

    println!("Transaciton {:#?} confirmed!", receipt.transaction_hash);

    Ok(receipt)
}

pub(crate) async fn extract_and_wait_for_priority_ops(
    receipt: TransactionReceipt,
    expected_diamond_proxy: Address,
    gateway_provider: Arc<Provider<Http>>,
) -> anyhow::Result<Vec<H256>> {
    let priority_ops = extract_priority_ops(receipt, expected_diamond_proxy).await?;

    logger::info(format!(
        "Migration has produced a total of {} priority operations for Gateway",
        priority_ops.len()
    ));
    for hash in priority_ops.iter() {
        await_for_tx_to_complete(&gateway_provider, *hash).await?;
    }

    Ok(priority_ops)
}

pub(crate) async fn extract_priority_ops(
    receipt: TransactionReceipt,
    expected_diamond_proxy: Address,
) -> anyhow::Result<Vec<H256>> {
    // TODO(EVM-749): cleanup the constant and automate its derivation
    let expected_topic_0: H256 = "4531cd5795773d7101c17bdeb9f5ab7f47d7056017506f937083be5d6e77a382"
        .parse()
        .unwrap();

    let priority_ops = receipt
        .logs
        .into_iter()
        .filter_map(|log| {
            if log.topics.is_empty() || log.topics[0] != expected_topic_0 {
                return None;
            }
            if log.address != expected_diamond_proxy {
                return None;
            }

            Some(H256::from_slice(&log.data[32..64]))
        })
        .collect();

    Ok(priority_ops)
}
