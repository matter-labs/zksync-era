//! WS-related tests.

use assert_matches::assert_matches;
use async_trait::async_trait;
use tokio::sync::watch;

use zksync_config::configs::chain::NetworkConfig;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{transactions_dal::L2TxSubmissionResult, ConnectionPool};
use zksync_types::{
    api, block::MiniblockHeader, fee::TransactionExecutionMetrics, tx::IncludedTxLocation, Address,
    L1BatchNumber, ProtocolVersionId, VmEvent, H256, U64,
};
use zksync_web3_decl::{
    jsonrpsee::{
        core::{
            client::{Subscription, SubscriptionClientT},
            Error as RpcError,
        },
        rpc_params,
        types::error::ErrorCode,
        ws_client::{WsClient, WsClientBuilder},
    },
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
    types::{BlockHeader, Filter, FilterChanges, PubSubFilter},
};

use super::*;
use crate::api_server::web3::metrics::SubscriptionType;

fn create_miniblock(number: u32) -> MiniblockHeader {
    MiniblockHeader {
        number: MiniblockNumber(number),
        timestamp: number.into(),
        hash: H256::from_low_u64_be(number.into()),
        l1_tx_count: 0,
        l2_tx_count: 0,
        base_fee_per_gas: 100,
        l1_gas_price: 100,
        l2_fair_gas_price: 100,
        base_system_contracts_hashes: BaseSystemContractsHashes::default(),
        protocol_version: Some(ProtocolVersionId::latest()),
        virtual_blocks: 1,
    }
}

#[allow(clippy::needless_pass_by_ref_mut)] // false positive
async fn wait_for_subscription(
    events: &mut mpsc::UnboundedReceiver<PubSubEvent>,
    sub_type: SubscriptionType,
) {
    let wait_future = tokio::time::timeout(TEST_TIMEOUT, async {
        loop {
            let event = events
                .recv()
                .await
                .expect("Events emitter unexpectedly dropped");
            if matches!(event, PubSubEvent::Subscribed(ty) if ty == sub_type) {
                break;
            } else {
                tracing::trace!(?event, "Skipping event");
            }
        }
    });
    wait_future
        .await
        .expect("Timed out waiting for subscription")
}

#[allow(clippy::needless_pass_by_ref_mut)] // false positive
async fn wait_for_notifier(
    events: &mut mpsc::UnboundedReceiver<PubSubEvent>,
    sub_type: SubscriptionType,
) {
    let wait_future = tokio::time::timeout(TEST_TIMEOUT, async {
        loop {
            let event = events
                .recv()
                .await
                .expect("Events emitter unexpectedly dropped");
            if matches!(event, PubSubEvent::NotifyIterationFinished(ty) if ty == sub_type) {
                break;
            } else {
                tracing::trace!(?event, "Skipping event");
            }
        }
    });
    wait_future.await.expect("Timed out waiting for notifier")
}

#[async_trait]
trait WsTest {
    async fn test(
        &self,
        client: &WsClient,
        pool: &ConnectionPool,
        pub_sub_events: mpsc::UnboundedReceiver<PubSubEvent>,
    ) -> anyhow::Result<()>;
}

async fn test_ws_server(test: impl WsTest) {
    let pool = ConnectionPool::test_pool().await;
    let network_config = NetworkConfig::for_tests();
    let mut storage = pool.access_storage().await.unwrap();
    if storage.blocks_dal().is_genesis_needed().await.unwrap() {
        ensure_genesis_state(
            &mut storage,
            network_config.zksync_network_id,
            &GenesisParams::mock(),
        )
        .await
        .unwrap();
    }
    drop(storage);

    let (stop_sender, stop_receiver) = watch::channel(false);
    let (server_handles, pub_sub_events) =
        spawn_ws_server(&network_config, pool.clone(), stop_receiver).await;
    server_handles.wait_until_ready().await;

    let client = WsClientBuilder::default()
        .build(format!("ws://{}", server_handles.local_addr))
        .await
        .unwrap();
    test.test(&client, &pool, pub_sub_events).await.unwrap();

    stop_sender.send_replace(true);
    server_handles.shutdown().await;
}

#[derive(Debug)]
struct WsServerCanStart;

#[async_trait]
impl WsTest for WsServerCanStart {
    async fn test(
        &self,
        client: &WsClient,
        _pool: &ConnectionPool,
        _pub_sub_events: mpsc::UnboundedReceiver<PubSubEvent>,
    ) -> anyhow::Result<()> {
        let block_number = client.get_block_number().await?;
        assert_eq!(block_number, U64::from(0));

        let l1_batch_number = client.get_l1_batch_number().await?;
        assert_eq!(l1_batch_number, U64::from(0));

        let genesis_l1_batch = client
            .get_l1_batch_details(L1BatchNumber(0))
            .await?
            .context("missing genesis L1 batch")?;
        assert!(genesis_l1_batch.base.root_hash.is_some());
        Ok(())
    }
}

#[tokio::test]
async fn ws_server_can_start() {
    test_ws_server(WsServerCanStart).await;
}

#[derive(Debug)]
struct BasicSubscriptions;

impl BasicSubscriptions {
    async fn update_storage(pool: &ConnectionPool) -> anyhow::Result<(MiniblockHeader, H256)> {
        let mut storage = pool.access_storage().await?;
        let new_tx = create_l2_transaction(1, 2);
        let new_tx_hash = new_tx.hash();
        let tx_submission_result = storage
            .transactions_dal()
            .insert_transaction_l2(new_tx, TransactionExecutionMetrics::default())
            .await;
        assert_matches!(tx_submission_result, L2TxSubmissionResult::Added);

        let new_miniblock = create_miniblock(1);
        storage
            .blocks_dal()
            .insert_miniblock(&new_miniblock)
            .await?;
        Ok((new_miniblock, new_tx_hash))
    }
}

#[async_trait]
impl WsTest for BasicSubscriptions {
    async fn test(
        &self,
        client: &WsClient,
        pool: &ConnectionPool,
        mut pub_sub_events: mpsc::UnboundedReceiver<PubSubEvent>,
    ) -> anyhow::Result<()> {
        // Wait for the notifiers to get initialized so that they don't skip notifications
        // for the created subscriptions.
        wait_for_notifier(&mut pub_sub_events, SubscriptionType::Blocks).await;
        wait_for_notifier(&mut pub_sub_events, SubscriptionType::Txs).await;

        let params = rpc_params!["newHeads"];
        let mut blocks_subscription = client
            .subscribe::<BlockHeader, _>("eth_subscribe", params, "eth_unsubscribe")
            .await?;
        wait_for_subscription(&mut pub_sub_events, SubscriptionType::Blocks).await;

        let params = rpc_params!["newPendingTransactions"];
        let mut txs_subscription = client
            .subscribe::<H256, _>("eth_subscribe", params, "eth_unsubscribe")
            .await?;
        wait_for_subscription(&mut pub_sub_events, SubscriptionType::Txs).await;

        let (new_miniblock, new_tx_hash) = Self::update_storage(pool).await?;

        let received_tx_hash = tokio::time::timeout(TEST_TIMEOUT, txs_subscription.next())
            .await
            .context("Timed out waiting for new tx hash")?
            .context("Pending txs subscription terminated")??;
        assert_eq!(received_tx_hash, new_tx_hash);
        let received_block_header = tokio::time::timeout(TEST_TIMEOUT, blocks_subscription.next())
            .await
            .context("Timed out waiting for new block hash")?
            .context("New blocks subscription terminated")??;
        assert_eq!(received_block_header.number, Some(1.into()));
        assert_eq!(received_block_header.hash, Some(new_miniblock.hash));
        assert_eq!(received_block_header.timestamp, 1.into());
        blocks_subscription.unsubscribe().await?;
        Ok(())
    }
}

#[tokio::test]
async fn basic_subscriptions() {
    test_ws_server(BasicSubscriptions).await;
}

#[derive(Debug)]
struct LogSubscriptions;

impl LogSubscriptions {
    async fn update_storage(
        storage: &mut StorageProcessor<'_>,
        miniblock_number: u32,
        start_idx: u32,
    ) -> anyhow::Result<(IncludedTxLocation, Vec<VmEvent>)> {
        let new_miniblock = create_miniblock(miniblock_number);
        storage
            .blocks_dal()
            .insert_miniblock(&new_miniblock)
            .await?;
        let tx_location = IncludedTxLocation {
            tx_hash: H256::repeat_byte(1),
            tx_index_in_miniblock: 0,
            tx_initiator_address: Address::repeat_byte(2),
        };
        let events = vec![
            // Matches address, doesn't match topics
            VmEvent {
                location: (L1BatchNumber(1), start_idx),
                address: Address::repeat_byte(23),
                indexed_topics: vec![],
                value: start_idx.to_le_bytes().to_vec(),
            },
            // Doesn't match address, matches topics
            VmEvent {
                location: (L1BatchNumber(1), start_idx + 1),
                address: Address::zero(),
                indexed_topics: vec![H256::repeat_byte(42)],
                value: (start_idx + 1).to_le_bytes().to_vec(),
            },
            // Doesn't match address or topics
            VmEvent {
                location: (L1BatchNumber(1), start_idx + 2),
                address: Address::zero(),
                indexed_topics: vec![H256::repeat_byte(1), H256::repeat_byte(42)],
                value: (start_idx + 2).to_le_bytes().to_vec(),
            },
            // Matches both address and topics
            VmEvent {
                location: (L1BatchNumber(1), start_idx + 3),
                address: Address::repeat_byte(23),
                indexed_topics: vec![H256::repeat_byte(42), H256::repeat_byte(111)],
                value: (start_idx + 3).to_le_bytes().to_vec(),
            },
        ];
        storage
            .events_dal()
            .save_events(
                MiniblockNumber(miniblock_number),
                &[(tx_location, events.iter().collect())],
            )
            .await;
        Ok((tx_location, events))
    }
}

#[derive(Debug)]
struct Subscriptions {
    all_logs_subscription: Subscription<api::Log>,
    address_subscription: Subscription<api::Log>,
    topic_subscription: Subscription<api::Log>,
}

impl Subscriptions {
    async fn new(
        client: &WsClient,
        pub_sub_events: &mut mpsc::UnboundedReceiver<PubSubEvent>,
    ) -> anyhow::Result<Self> {
        // Wait for the notifier to get initialized so that it doesn't skip notifications
        // for the created subscriptions.
        wait_for_notifier(pub_sub_events, SubscriptionType::Logs).await;

        let params = rpc_params!["logs"];
        let all_logs_subscription = client
            .subscribe::<api::Log, _>("eth_subscribe", params, "eth_unsubscribe")
            .await?;
        let address_filter = PubSubFilter {
            address: Some(Address::repeat_byte(23).into()),
            topics: None,
        };
        let params = rpc_params!["logs", address_filter];
        let address_subscription = client
            .subscribe::<api::Log, _>("eth_subscribe", params, "eth_unsubscribe")
            .await?;
        let topic_filter = PubSubFilter {
            address: None,
            topics: Some(vec![Some(H256::repeat_byte(42).into())]),
        };
        let params = rpc_params!["logs", topic_filter];
        let topic_subscription = client
            .subscribe::<api::Log, _>("eth_subscribe", params, "eth_unsubscribe")
            .await?;
        for _ in 0..3 {
            wait_for_subscription(pub_sub_events, SubscriptionType::Logs).await;
        }

        Ok(Self {
            all_logs_subscription,
            address_subscription,
            topic_subscription,
        })
    }
}

#[async_trait]
impl WsTest for LogSubscriptions {
    async fn test(
        &self,
        client: &WsClient,
        pool: &ConnectionPool,
        mut pub_sub_events: mpsc::UnboundedReceiver<PubSubEvent>,
    ) -> anyhow::Result<()> {
        let Subscriptions {
            mut all_logs_subscription,
            mut address_subscription,
            mut topic_subscription,
        } = Subscriptions::new(client, &mut pub_sub_events).await?;

        let mut storage = pool.access_storage().await?;
        let (tx_location, events) = Self::update_storage(&mut storage, 1, 0).await?;
        drop(storage);
        let events: Vec<_> = events.iter().collect();

        let all_logs = collect_logs(&mut all_logs_subscription, 4).await?;
        for (i, log) in all_logs.iter().enumerate() {
            assert_eq!(log.transaction_index, Some(0.into()));
            assert_eq!(log.log_index, Some(i.into()));
            assert_eq!(log.transaction_hash, Some(tx_location.tx_hash));
            assert_eq!(log.block_number, Some(1.into()));
        }
        assert_logs_match(&all_logs, &events);

        let address_logs = collect_logs(&mut address_subscription, 2).await?;
        assert_logs_match(&address_logs, &[events[0], events[3]]);

        let topic_logs = collect_logs(&mut topic_subscription, 2).await?;
        assert_logs_match(&topic_logs, &[events[1], events[3]]);

        wait_for_notifier(&mut pub_sub_events, SubscriptionType::Logs).await;

        // Check that no new notifications were sent to subscribers.
        tokio::time::timeout(POLL_INTERVAL, all_logs_subscription.next())
            .await
            .unwrap_err();
        tokio::time::timeout(POLL_INTERVAL, address_subscription.next())
            .await
            .unwrap_err();
        tokio::time::timeout(POLL_INTERVAL, topic_subscription.next())
            .await
            .unwrap_err();
        Ok(())
    }
}

async fn collect_logs(
    sub: &mut Subscription<api::Log>,
    expected_count: usize,
) -> anyhow::Result<Vec<api::Log>> {
    let mut logs = Vec::with_capacity(expected_count);
    for _ in 0..expected_count {
        let log = tokio::time::timeout(TEST_TIMEOUT, sub.next())
            .await
            .context("Timed out waiting for new log")?
            .context("Logs subscription terminated")??;
        logs.push(log);
    }
    Ok(logs)
}

fn assert_logs_match(actual_logs: &[api::Log], expected_logs: &[&VmEvent]) {
    assert_eq!(actual_logs.len(), expected_logs.len());
    for (actual_log, &expected_log) in actual_logs.iter().zip(expected_logs) {
        assert_eq!(actual_log.address, expected_log.address);
        assert_eq!(actual_log.topics, expected_log.indexed_topics);
        assert_eq!(actual_log.data.0, expected_log.value);
    }
}

#[tokio::test]
async fn log_subscriptions() {
    test_ws_server(LogSubscriptions).await;
}

#[derive(Debug)]
struct LogSubscriptionsWithNewBlock;

#[async_trait]
impl WsTest for LogSubscriptionsWithNewBlock {
    async fn test(
        &self,
        client: &WsClient,
        pool: &ConnectionPool,
        mut pub_sub_events: mpsc::UnboundedReceiver<PubSubEvent>,
    ) -> anyhow::Result<()> {
        let Subscriptions {
            mut all_logs_subscription,
            mut address_subscription,
            ..
        } = Subscriptions::new(client, &mut pub_sub_events).await?;

        let mut storage = pool.access_storage().await?;
        let (_, events) = LogSubscriptions::update_storage(&mut storage, 1, 0).await?;
        drop(storage);
        let events: Vec<_> = events.iter().collect();

        let all_logs = collect_logs(&mut all_logs_subscription, 4).await?;
        assert_logs_match(&all_logs, &events);

        // Create a new block and wait for the pub-sub notifier to run.
        let mut storage = pool.access_storage().await?;
        let (_, new_events) = LogSubscriptions::update_storage(&mut storage, 2, 4).await?;
        drop(storage);
        let new_events: Vec<_> = new_events.iter().collect();

        let all_new_logs = collect_logs(&mut all_logs_subscription, 4).await?;
        assert_logs_match(&all_new_logs, &new_events);

        let address_logs = collect_logs(&mut address_subscription, 4).await?;
        assert_logs_match(
            &address_logs,
            &[events[0], events[3], new_events[0], new_events[3]],
        );
        Ok(())
    }
}

#[tokio::test]
async fn log_subscriptions_with_new_block() {
    test_ws_server(LogSubscriptionsWithNewBlock).await;
}

#[derive(Debug)]
struct LogSubscriptionsWithManyBlocks;

#[async_trait]
impl WsTest for LogSubscriptionsWithManyBlocks {
    async fn test(
        &self,
        client: &WsClient,
        pool: &ConnectionPool,
        mut pub_sub_events: mpsc::UnboundedReceiver<PubSubEvent>,
    ) -> anyhow::Result<()> {
        let Subscriptions {
            mut all_logs_subscription,
            mut address_subscription,
            ..
        } = Subscriptions::new(client, &mut pub_sub_events).await?;

        // Add two blocks in the storage atomically.
        let mut storage = pool.access_storage().await?;
        let mut transaction = storage.start_transaction().await?;
        let (_, events) = LogSubscriptions::update_storage(&mut transaction, 1, 0).await?;
        let events: Vec<_> = events.iter().collect();
        let (_, new_events) = LogSubscriptions::update_storage(&mut transaction, 2, 4).await?;
        let new_events: Vec<_> = new_events.iter().collect();
        transaction.commit().await?;
        drop(storage);

        let all_logs = collect_logs(&mut all_logs_subscription, 4).await?;
        assert_logs_match(&all_logs, &events);
        let all_new_logs = collect_logs(&mut all_logs_subscription, 4).await?;
        assert_logs_match(&all_new_logs, &new_events);

        let address_logs = collect_logs(&mut address_subscription, 4).await?;
        assert_logs_match(
            &address_logs,
            &[events[0], events[3], new_events[0], new_events[3]],
        );
        Ok(())
    }
}

#[tokio::test]
async fn log_subscriptions_with_many_new_blocks_at_once() {
    test_ws_server(LogSubscriptionsWithManyBlocks).await;
}

#[derive(Debug)]
struct LogSubscriptionsWithDelay;

#[async_trait]
impl WsTest for LogSubscriptionsWithDelay {
    async fn test(
        &self,
        client: &WsClient,
        pool: &ConnectionPool,
        mut pub_sub_events: mpsc::UnboundedReceiver<PubSubEvent>,
    ) -> anyhow::Result<()> {
        // Store a miniblock w/o subscriptions being present.
        let mut storage = pool.access_storage().await?;
        LogSubscriptions::update_storage(&mut storage, 1, 0).await?;
        drop(storage);

        while pub_sub_events.try_recv().is_ok() {
            // Drain all existing pub-sub events.
        }
        wait_for_notifier(&mut pub_sub_events, SubscriptionType::Logs).await;

        let params = rpc_params!["logs"];
        let mut all_logs_subscription = client
            .subscribe::<api::Log, _>("eth_subscribe", params, "eth_unsubscribe")
            .await?;
        let address_and_topic_filter = PubSubFilter {
            address: Some(Address::repeat_byte(23).into()),
            topics: Some(vec![Some(H256::repeat_byte(42).into())]),
        };
        let params = rpc_params!["logs", address_and_topic_filter];
        let mut address_and_topic_subscription = client
            .subscribe::<api::Log, _>("eth_subscribe", params, "eth_unsubscribe")
            .await?;
        for _ in 0..2 {
            wait_for_subscription(&mut pub_sub_events, SubscriptionType::Logs).await;
        }

        let mut storage = pool.access_storage().await?;
        let (_, new_events) = LogSubscriptions::update_storage(&mut storage, 2, 4).await?;
        drop(storage);
        let new_events: Vec<_> = new_events.iter().collect();

        let all_logs = collect_logs(&mut all_logs_subscription, 4).await?;
        assert_logs_match(&all_logs, &new_events);
        let address_and_topic_logs = collect_logs(&mut address_and_topic_subscription, 1).await?;
        assert_logs_match(&address_and_topic_logs, &[new_events[3]]);

        // Check the behavior of remaining subscriptions if a subscription is dropped.
        all_logs_subscription.unsubscribe().await?;
        let mut storage = pool.access_storage().await?;
        let (_, new_events) = LogSubscriptions::update_storage(&mut storage, 3, 8).await?;
        drop(storage);

        let address_and_topic_logs = collect_logs(&mut address_and_topic_subscription, 1).await?;
        assert_logs_match(&address_and_topic_logs, &[&new_events[3]]);
        Ok(())
    }
}

#[tokio::test]
async fn log_subscriptions_with_delay() {
    test_ws_server(LogSubscriptionsWithDelay).await;
}

#[derive(Debug)]
struct BasicFilterChanges;

#[async_trait]
impl WsTest for BasicFilterChanges {
    async fn test(
        &self,
        client: &WsClient,
        pool: &ConnectionPool,
        _pub_sub_events: mpsc::UnboundedReceiver<PubSubEvent>,
    ) -> anyhow::Result<()> {
        let block_filter_id = client.new_block_filter().await?;
        let tx_filter_id = client.new_pending_transaction_filter().await?;

        let (new_miniblock, new_tx_hash) = BasicSubscriptions::update_storage(pool).await?;

        let block_filter_changes = client.get_filter_changes(block_filter_id).await?;
        assert_matches!(
            block_filter_changes,
            FilterChanges::Hashes(hashes) if hashes == [new_miniblock.hash]
        );
        let block_filter_changes = client.get_filter_changes(block_filter_id).await?;
        assert_matches!(block_filter_changes, FilterChanges::Hashes(hashes) if hashes.is_empty());

        let tx_filter_changes = client.get_filter_changes(tx_filter_id).await?;
        assert_matches!(
            tx_filter_changes,
            FilterChanges::Hashes(hashes) if hashes == [new_tx_hash]
        );
        let tx_filter_changes = client.get_filter_changes(tx_filter_id).await?;
        assert_matches!(tx_filter_changes, FilterChanges::Hashes(hashes) if hashes.is_empty());

        // Check uninstalling the filter.
        let removed = client.uninstall_filter(block_filter_id).await?;
        assert!(removed);
        let removed = client.uninstall_filter(block_filter_id).await?;
        assert!(!removed);

        let err = client
            .get_filter_changes(block_filter_id)
            .await
            .unwrap_err();
        assert_matches!(err, RpcError::Call(err) if err.code() == ErrorCode::InvalidParams.code());
        Ok(())
    }
}

#[tokio::test]
async fn basic_filter_changes() {
    test_ws_server(BasicFilterChanges).await;
}

#[derive(Debug)]
struct LogFilterChanges;

#[async_trait]
impl WsTest for LogFilterChanges {
    async fn test(
        &self,
        client: &WsClient,
        pool: &ConnectionPool,
        _pub_sub_events: mpsc::UnboundedReceiver<PubSubEvent>,
    ) -> anyhow::Result<()> {
        let all_logs_filter_id = client.new_filter(Filter::default()).await?;
        let address_filter = Filter {
            address: Some(Address::repeat_byte(23).into()),
            ..Filter::default()
        };
        let address_filter_id = client.new_filter(address_filter).await?;
        let topics_filter = Filter {
            topics: Some(vec![Some(H256::repeat_byte(42).into())]),
            ..Filter::default()
        };
        let topics_filter_id = client.new_filter(topics_filter).await?;

        let mut storage = pool.access_storage().await?;
        let (_, events) = LogSubscriptions::update_storage(&mut storage, 1, 0).await?;
        drop(storage);
        let events: Vec<_> = events.iter().collect();

        let all_logs = client.get_filter_changes(all_logs_filter_id).await?;
        let FilterChanges::Logs(all_logs) = all_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", all_logs);
        };
        assert_logs_match(&all_logs, &events);

        let address_logs = client.get_filter_changes(address_filter_id).await?;
        let FilterChanges::Logs(address_logs) = address_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", address_logs);
        };
        assert_logs_match(&address_logs, &[events[0], events[3]]);

        let topics_logs = client.get_filter_changes(topics_filter_id).await?;
        let FilterChanges::Logs(topics_logs) = topics_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", topics_logs);
        };
        assert_logs_match(&topics_logs, &[events[1], events[3]]);

        let new_all_logs = client.get_filter_changes(all_logs_filter_id).await?;
        let FilterChanges::Logs(new_all_logs) = new_all_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", new_all_logs);
        };
        assert_eq!(new_all_logs, all_logs); // FIXME: is this expected?
        Ok(())
    }
}

#[tokio::test]
async fn log_filter_changes() {
    test_ws_server(LogFilterChanges).await;
}

#[derive(Debug)]
struct LogFilterChangesWithBlockBoundaries;

#[async_trait]
impl WsTest for LogFilterChangesWithBlockBoundaries {
    async fn test(
        &self,
        client: &WsClient,
        pool: &ConnectionPool,
        _pub_sub_events: mpsc::UnboundedReceiver<PubSubEvent>,
    ) -> anyhow::Result<()> {
        let lower_bound_filter = Filter {
            from_block: Some(api::BlockNumber::Number(2.into())),
            ..Filter::default()
        };
        let lower_bound_filter_id = client.new_filter(lower_bound_filter).await?;
        let upper_bound_filter = Filter {
            to_block: Some(api::BlockNumber::Number(1.into())),
            ..Filter::default()
        };
        let upper_bound_filter_id = client.new_filter(upper_bound_filter).await?;
        let bounded_filter = Filter {
            from_block: Some(api::BlockNumber::Number(1.into())),
            to_block: Some(api::BlockNumber::Number(1.into())),
            ..Filter::default()
        };
        let bounded_filter_id = client.new_filter(bounded_filter).await?;

        let mut storage = pool.access_storage().await?;
        let (_, events) = LogSubscriptions::update_storage(&mut storage, 1, 0).await?;
        drop(storage);
        let events: Vec<_> = events.iter().collect();

        let lower_bound_logs = client.get_filter_changes(lower_bound_filter_id).await?;
        assert_matches!(
            lower_bound_logs,
            FilterChanges::Hashes(hashes) if hashes.is_empty()
        );
        // ^ Since `FilterChanges` is serialized w/o a tag, an empty array will be deserialized
        // as `Hashes(_)` (the first declared variant).

        let upper_bound_logs = client.get_filter_changes(upper_bound_filter_id).await?;
        let FilterChanges::Logs(upper_bound_logs) = upper_bound_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", upper_bound_logs);
        };
        assert_logs_match(&upper_bound_logs, &events);
        let bounded_logs = client.get_filter_changes(bounded_filter_id).await?;
        let FilterChanges::Logs(bounded_logs) = bounded_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", bounded_logs);
        };
        assert_eq!(bounded_logs, upper_bound_logs);

        // Add another miniblock with events to the storage.
        let mut storage = pool.access_storage().await?;
        let (_, new_events) = LogSubscriptions::update_storage(&mut storage, 2, 4).await?;
        drop(storage);
        let new_events: Vec<_> = new_events.iter().collect();

        let lower_bound_logs = client.get_filter_changes(lower_bound_filter_id).await?;
        let FilterChanges::Logs(lower_bound_logs) = lower_bound_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", lower_bound_logs);
        };
        assert_logs_match(&lower_bound_logs, &new_events);

        // FIXME: is this expected?
        let new_upper_bound_logs = client.get_filter_changes(upper_bound_filter_id).await?;
        assert_eq!(new_upper_bound_logs, FilterChanges::Logs(upper_bound_logs));
        let new_bounded_logs = client.get_filter_changes(bounded_filter_id).await?;
        assert_eq!(new_bounded_logs, FilterChanges::Logs(bounded_logs));

        // Add miniblock #3. It should not be picked up by the bounded and upper bound filters,
        // and should be picked up by the lower bound filter.
        let mut storage = pool.access_storage().await?;
        let (_, new_events) = LogSubscriptions::update_storage(&mut storage, 3, 8).await?;
        drop(storage);
        let new_events: Vec<_> = new_events.iter().collect();

        let bounded_logs = client.get_filter_changes(bounded_filter_id).await?;
        let FilterChanges::Logs(bounded_logs) = bounded_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", bounded_logs);
        };
        assert!(bounded_logs
            .iter()
            .all(|log| log.block_number.unwrap() < 3.into()));

        let upper_bound_logs = client.get_filter_changes(upper_bound_filter_id).await?;
        let FilterChanges::Logs(upper_bound_logs) = upper_bound_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", upper_bound_logs);
        };
        assert!(upper_bound_logs
            .iter()
            .all(|log| log.block_number.unwrap() < 3.into()));

        let lower_bound_logs = client.get_filter_changes(lower_bound_filter_id).await?;
        let FilterChanges::Logs(lower_bound_logs) = lower_bound_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", lower_bound_logs);
        };
        let start_idx = lower_bound_logs.len() - 4;
        assert_logs_match(&lower_bound_logs[start_idx..], &new_events);
        Ok(())
    }
}

#[tokio::test]
async fn log_filter_changes_with_block_boundaries() {
    test_ws_server(LogFilterChangesWithBlockBoundaries).await;
}
