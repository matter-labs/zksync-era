// src/commands/chain/migrate_to_gateway_calldata.rs

use std::sync::Arc;

use anyhow::Context;
use chrono::Utc;
use ethers::{
    providers::{Http, Middleware, Provider},
    types::{Filter, TransactionReceipt},
    utils::keccak256,
};
use xshell::Shell;
use zkstack_cli_common::{
    ethereum::{get_ethers_provider, get_zk_client},
    forge::ForgeScriptArgs,
    logger,
};
use zkstack_cli_config::{ZkStackConfig, ZkStackConfigTrait};
use zksync_basic_types::{Address, H256, U256, U64};
use zksync_system_constants::L2_BRIDGEHUB_ADDRESS;
use zksync_types::{
    server_notification::{GatewayMigrationNotification, GatewayMigrationState},
    settlement::SettlementLayer,
    u256_to_h256,
};
use zksync_web3_decl::namespaces::UnstableNamespaceClient;

use super::{
    migrate_from_gateway::check_whether_gw_transaction_is_finalized,
    notify_server_calldata::{get_notify_server_calls, NotifyServerCallsArgs},
};
use crate::{
    abi::{BridgehubAbi, ChainTypeManagerAbi, IChainAssetHandlerAbi, ZkChainAbi},
    commands::chain::{admin_call_builder::AdminCallBuilder, utils::send_tx},
    consts::DEFAULT_EVENTS_BLOCK_RANGE,
};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MigrationDirection {
    FromGateway,
    ToGateway,
}

impl MigrationDirection {
    pub(crate) fn expected_notificaation(self) -> GatewayMigrationNotification {
        match self {
            Self::FromGateway => GatewayMigrationNotification::FromGateway,
            Self::ToGateway => GatewayMigrationNotification::ToGateway,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum NotificationReceivedState {
    NotAllBatchesExecuted(U256, U256),
    UnconfirmedTxs(usize),
}

impl std::fmt::Display for NotificationReceivedState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            NotificationReceivedState::NotAllBatchesExecuted(
                total_batches_committed,
                total_batches_executed,
            ) => {
                write!(f, "For migration from Gateway we need all batches to be executed. Executed/committed: {total_batches_executed}/{total_batches_committed}")
            }
            NotificationReceivedState::UnconfirmedTxs(unconfirmed_txs) => {
                write!(
                    f,
                    "There are some unconfirmed transactions: {unconfirmed_txs}"
                )
            }
        }
    }
}

/// Each migration to or from ZK gateway has multiple states it can be in.
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum GatewayMigrationProgressState {
    /// The state that represents that the migration has not yet started.
    NotStarted,
    /// The chain admin has sent the notification
    NotificationSent,
    /// The server has received the notification, but it is not yet ready for the migration.
    NotificationReceived(NotificationReceivedState),
    /// The server has received the notification and has no pending transactions
    ServerReady,
    /// The server is ready and the migration has started, but the server has not started sending transactions
    /// to the new settlement layer yet.
    AwaitingFinalization,
    /// (Only for migrations from Gateway). The migration has been finalized on L1, but the user needs to execute it.
    PendingManualFinalization,
    /// The migration has finished.
    Finished,
}

const MAX_SEARCHING_MIGRATION_TXS_INTERVAL: chrono::Duration = chrono::Duration::days(5);

pub(crate) async fn get_migration_transaction(
    sl_rpc_url: &str,
    bridgehub_address: Address,
    l2_chain_id: u64,
) -> anyhow::Result<Option<H256>> {
    let provider = get_ethers_provider(sl_rpc_url)?;
    let sl_chain_id = provider.get_chainid().await?;

    logger::info(format!(
        "Searching for the migration transaction on SL {:#?}...",
        sl_chain_id
    ));

    // Get the latest block so we know how far we can go
    let mut search_upper_bound = provider
        .get_block_number()
        .await
        .expect("Failed to fetch latest block")
        .as_u64();

    let bridgehub = BridgehubAbi::new(bridgehub_address, provider.clone());
    let chain_asset_handler_addr = bridgehub.chain_asset_handler().await?;
    let chain_asset_handler =
        IChainAssetHandlerAbi::new(chain_asset_handler_addr, provider.clone());

    let max_interval_to_search = Utc::now() - MAX_SEARCHING_MIGRATION_TXS_INTERVAL;
    let latest_tx_hash: Option<H256> = loop {
        let lower_bound = search_upper_bound.saturating_sub(DEFAULT_EVENTS_BLOCK_RANGE);

        logger::info(format!(
            "Checking block range: {}..={}",
            lower_bound, search_upper_bound
        ));

        let ev = chain_asset_handler
            .migration_started_filter()
            .topic1(U256::from(l2_chain_id))
            .from_block(lower_bound)
            .to_block(search_upper_bound);

        let results = ev.query_with_meta().await?;
        if let Some((_, meta)) = results.last() {
            break Some(meta.transaction_hash);
        }

        if lower_bound == 0 {
            break None;
        }

        let block_info = provider.get_block(lower_bound).await?.unwrap();
        if block_info
            .time()
            .expect("Can not represent block.timestamp as DateTime<UTC>")
            < max_interval_to_search
        {
            break None;
        }

        search_upper_bound = lower_bound - 1;
    };

    Ok(latest_tx_hash)
}

async fn get_batch_execution_status(
    sl_rpc_url: &str,
    bridgehub_address: Address,
    l2_chain_id: u64,
) -> anyhow::Result<(U256, U256)> {
    let provider = get_ethers_provider(sl_rpc_url)?;
    let sl_bridgehub = BridgehubAbi::new(bridgehub_address, provider.clone());
    let zk_chain_address = sl_bridgehub.get_zk_chain(U256::from(l2_chain_id)).await?;
    let zk_chain = ZkChainAbi::new(zk_chain_address, provider);
    let total_committed = zk_chain.get_total_batches_committed().await?;
    let total_executed = zk_chain.get_total_batches_committed().await?;

    Ok((total_committed, total_executed))
}

pub(crate) async fn get_gateway_migration_state(
    l1_rpc_url: String,
    l1_bridgehub_addr: Address,
    l2_chain_id: u64,
    l2_rpc_url: String,
    gw_rpc_url: String,
    direction: MigrationDirection,
) -> anyhow::Result<GatewayMigrationProgressState> {
    let l1_provider = get_ethers_provider(&l1_rpc_url)?;
    let l1_chain_id = l1_provider.get_chainid().await?.as_u64();
    let l1_bridgehub = BridgehubAbi::new(l1_bridgehub_addr, l1_provider.clone());

    let l1_ctm_address = l1_bridgehub.chain_type_manager(l2_chain_id.into()).await?;
    let l1_ctm = ChainTypeManagerAbi::new(l1_ctm_address, l1_provider.clone());

    let current_sl_from_l1 = l1_bridgehub
        .settlement_layer(l2_chain_id.into())
        .await?
        .as_u64();

    let zk_client = get_zk_client(&l2_rpc_url, l2_chain_id)?;

    let gateway_migration_status = match zk_client.gateway_migration_status().await {
        Ok(status) => status,
        Err(e) => {
            anyhow::bail!(format!("Failed to retrieve gateway migration status from the server. Error: {:#?} Ensure that the server supports this method and has `unstable` namespace turned on", e));
        }
    };

    // Firstly we check whether any event has been sent
    // It is expected that any migration starts with a notification, even though it is not enforced.
    let Some(latest_event) =
        get_latest_notification_event_from_l1(l2_chain_id, l1_ctm, l1_provider.clone()).await?
    else {
        logger::info("No gateway migration events found on L1");

        // No event has been sent.
        // It means that either migration has not yet started or
        // the chain admin has completed the migration but without sending any notiifcation.
        // Or the latest migration was long time ago.

        // Firslty check for consistency with the server.
        if gateway_migration_status.state == GatewayMigrationState::InProgress {
            anyhow::bail!("Very old migration in progress");
        }

        let Some(current_sl_from_server) = gateway_migration_status.settlement_layer else {
            // It means that the server is in the middle of some migration.
            anyhow::bail!("Very old migration in progress");
        };

        // No migration event present, but the server uses inconsistent settlement layer
        if current_sl_from_l1 != current_sl_from_server.chain_id().0 {
            anyhow::bail!(format!("No migration event present, but server uses inconsistent settlement layer. Server: {current_sl_from_server:?}, L1 Bridgehub: {current_sl_from_l1:?}"));
        }

        // The system does not have any migration at this point, we just need to check
        // whether it is `NotStarted` or `Finished` depending on the propoed direction

        let status = match (direction, current_sl_from_server) {
            (MigrationDirection::ToGateway, SettlementLayer::Gateway(_)) => {
                GatewayMigrationProgressState::Finished
            }
            (MigrationDirection::ToGateway, SettlementLayer::L1(_)) => {
                GatewayMigrationProgressState::NotStarted
            }
            (MigrationDirection::FromGateway, SettlementLayer::Gateway(_)) => {
                GatewayMigrationProgressState::NotStarted
            }
            (MigrationDirection::FromGateway, SettlementLayer::L1(_)) => {
                GatewayMigrationProgressState::Finished
            }
        };

        return Ok(status);
    };

    // Some event has been sent on L1.
    // It may be an event from previous migration or it may be related to the current, new one.
    let expected_notification = direction.expected_notificaation();

    if latest_event != expected_notification {
        // It is likely a leftover from a previous migration
        return Ok(GatewayMigrationProgressState::NotStarted);
    }

    // Now, we know that the last event is aligned with the migration direction.
    // Let's firstly double check whether the migration has finished.

    match (direction, gateway_migration_status.settlement_layer) {
        // The server already uses the new settlement layer, so the migration is over
        (MigrationDirection::ToGateway, Some(SettlementLayer::Gateway(_)))
        | (MigrationDirection::FromGateway, Some(SettlementLayer::L1(_))) => {
            return Ok(GatewayMigrationProgressState::Finished)
        }
        _ => {}
    };

    // Now we know that the migraiton has started, but it is in progress somehow

    // Let's check if the server has seen the event

    let Some(latest_server_notification) = gateway_migration_status.latest_notification else {
        // The server has seen no notification yet
        return Ok(GatewayMigrationProgressState::NotificationSent);
    };

    if latest_server_notification != latest_event {
        // The latest seen notification is from a different event
        return Ok(GatewayMigrationProgressState::NotificationSent);
    }

    // The server has seen the event, but does not use the new settlement layer yet,
    // let's do a small consistency check
    if gateway_migration_status.state != GatewayMigrationState::InProgress {
        anyhow::bail!("Server has seen notification, does not use the settlement layer, but still the migration is not in progress. Status: {:#?}", gateway_migration_status);
    }

    // For migration from Gateway we also require that all batches have been executed

    if direction == MigrationDirection::FromGateway {
        let (total_batches_committed, total_batches_executed) =
            get_batch_execution_status(&gw_rpc_url, L2_BRIDGEHUB_ADDRESS, l2_chain_id).await?;

        if total_batches_committed != total_batches_executed {
            // Server still waits for the batches to get executed
            return Ok(GatewayMigrationProgressState::NotificationReceived(
                NotificationReceivedState::NotAllBatchesExecuted(
                    total_batches_committed,
                    total_batches_executed,
                ),
            ));
        }
    }

    let unconfirmed_txs = zk_client.get_unconfirmed_txs_count().await?;

    if unconfirmed_txs != 0 {
        // The server has received the notification, but there are still some pending txs
        return Ok(GatewayMigrationProgressState::NotificationReceived(
            NotificationReceivedState::UnconfirmedTxs(unconfirmed_txs),
        ));
    }

    // Now we know that the server is ready, but we need to double check whether the migration has already
    // been finalized by the chain admin.

    if direction == MigrationDirection::ToGateway {
        if current_sl_from_l1 != l1_chain_id {
            return Ok(GatewayMigrationProgressState::AwaitingFinalization);
        }

        return Ok(GatewayMigrationProgressState::ServerReady);
    }

    let gw_provider = get_ethers_provider(&gw_rpc_url)?;
    let gw_bridgehub = BridgehubAbi::new(L2_BRIDGEHUB_ADDRESS, gw_provider.clone());
    let gw_chain_id = gw_provider.get_chainid().await?;

    let current_sl_from_gw = gw_bridgehub.settlement_layer(l2_chain_id.into()).await?;
    if current_sl_from_gw == U256::zero() {
        anyhow::bail!("Chain is not present on Gateway");
    }

    if current_sl_from_gw == gw_chain_id {
        // The migration has not been finalized by the admin
        return Ok(GatewayMigrationProgressState::ServerReady);
    }

    // Now we need to find an event where the migration has happened.
    let migration_transaction =
        get_migration_transaction(&gw_rpc_url, L2_BRIDGEHUB_ADDRESS, l2_chain_id)
            .await?
            .context("Can not find the migration transaction")?;
    logger::info(format!(
        "The migration transaction with hash {:#?} has been found",
        migration_transaction
    ));

    let gw_zk_client = get_zk_client(&gw_rpc_url, gw_chain_id.as_u64())?;
    let is_tx_finalized = check_whether_gw_transaction_is_finalized(
        &gw_zk_client,
        l1_provider,
        l1_bridgehub.get_zk_chain(gw_chain_id).await?,
        migration_transaction,
    )
    .await?;

    if !is_tx_finalized {
        logger::info("The migration transaction is not yet finalized.");
        // The transaction is not yet finalized.
        return Ok(GatewayMigrationProgressState::AwaitingFinalization);
    }

    // The batch with migration transaction has been finalized, we only need to finalize the "withdrawal" of the chain
    Ok(GatewayMigrationProgressState::PendingManualFinalization)
}

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
        .from_block(latest_block.saturating_sub(DEFAULT_EVENTS_BLOCK_RANGE))
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

pub(crate) async fn await_for_tx_to_complete(
    gateway_provider: &Arc<Provider<Http>>,
    hash: H256,
) -> anyhow::Result<()> {
    logger::info(format!(
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
        anyhow::bail!("Transaction failed! Receipt: {:?}", receipt);
    }

    Ok(())
}

pub(crate) async fn notify_server(
    args: ForgeScriptArgs,
    shell: &Shell,
    direction: MigrationDirection,
) -> anyhow::Result<()> {
    let chain_config = ZkStackConfig::current_chain(shell)?;

    let l1_url = chain_config.get_secrets_config().await?.l1_rpc_url()?;
    let contracts = chain_config.get_contracts_config()?;

    let calls = get_notify_server_calls(
        shell,
        &args,
        &chain_config.path_to_foundry_scripts(),
        NotifyServerCallsArgs {
            l1_bridgehub_addr: contracts.ecosystem_contracts.bridgehub_proxy_addr,
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
        "notifying server",
    )
    .await?;

    Ok(())
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
    let expected_topic_0: ethers::types::H256 = ethers::types::H256::from(keccak256(
        b"NewPriorityRequest(uint256,bytes32,uint64,(uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256[4],bytes,bytes,uint256[],bytes,bytes),bytes[])",
    ));

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
