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
use zksync_web3_decl::types::Filter;
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
    types::{BlockHeader, FilterChanges, PubSubFilter},
};

use super::*;

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

#[async_trait]
trait WsTest {
    async fn test(&self, client: &WsClient, pool: &ConnectionPool);
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
    let server_handles = spawn_server(
        ApiTransportLabel::Ws,
        &network_config,
        pool.clone(),
        stop_receiver,
    )
    .await;
    server_handles.wait_until_ready().await;

    let client = WsClientBuilder::default()
        .build(format!("ws://{}", server_handles.local_addr))
        .await
        .unwrap();
    test.test(&client, &pool).await;

    stop_sender.send_replace(true);
    server_handles.shutdown().await;
}

#[derive(Debug)]
struct WsServerCanStart;

#[async_trait]
impl WsTest for WsServerCanStart {
    async fn test(&self, client: &WsClient, _pool: &ConnectionPool) {
        let block_number = client.get_block_number().await.unwrap();
        assert_eq!(block_number, U64::from(0));

        let l1_batch_number = client.get_l1_batch_number().await.unwrap();
        assert_eq!(l1_batch_number, U64::from(0));

        let genesis_l1_batch = client
            .get_l1_batch_details(L1BatchNumber(0))
            .await
            .unwrap()
            .unwrap();
        assert!(genesis_l1_batch.base.root_hash.is_some());
    }
}

#[tokio::test]
async fn ws_server_can_start() {
    test_ws_server(WsServerCanStart).await;
}

#[derive(Debug)]
struct BasicSubscriptions;

impl BasicSubscriptions {
    async fn update_storage(pool: &ConnectionPool) -> (MiniblockHeader, H256) {
        let mut storage = pool.access_storage().await.unwrap();
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
            .await
            .unwrap();
        (new_miniblock, new_tx_hash)
    }
}

#[async_trait]
impl WsTest for BasicSubscriptions {
    async fn test(&self, client: &WsClient, pool: &ConnectionPool) {
        let params = rpc_params!["newHeads"];
        let mut blocks_subscription = client
            .subscribe::<BlockHeader, _>("eth_subscribe", params, "eth_unsubscribe")
            .await
            .unwrap();
        let params = rpc_params!["newPendingTransactions"];
        let mut txs_subscription = client
            .subscribe::<H256, _>("eth_subscribe", params, "eth_unsubscribe")
            .await
            .unwrap();

        let (new_miniblock, new_tx_hash) = Self::update_storage(pool).await;

        let received_tx_hash = tokio::time::timeout(TEST_TIMEOUT, txs_subscription.next())
            .await
            .expect("Timed out waiting for new tx hash")
            .expect("Pending txs subscription terminated")
            .unwrap();
        assert_eq!(received_tx_hash, new_tx_hash);
        let received_block_header = tokio::time::timeout(TEST_TIMEOUT, blocks_subscription.next())
            .await
            .expect("Timed out waiting for new block hash")
            .expect("New blocks subscription terminated")
            .unwrap();
        assert_eq!(received_block_header.number, Some(1.into()));
        assert_eq!(received_block_header.hash, Some(new_miniblock.hash));
        assert_eq!(received_block_header.timestamp, 1.into());
        blocks_subscription.unsubscribe().await.unwrap();
    }
}

#[tokio::test]
async fn basic_subscriptions() {
    test_ws_server(BasicSubscriptions).await;
}

#[derive(Debug)]
struct LogSubscriptions;

impl LogSubscriptions {
    async fn update_storage(pool: &ConnectionPool) -> (IncludedTxLocation, Vec<VmEvent>) {
        let mut storage = pool.access_storage().await.unwrap();
        let new_miniblock = create_miniblock(1);
        storage
            .blocks_dal()
            .insert_miniblock(&new_miniblock)
            .await
            .unwrap();
        let tx_location = IncludedTxLocation {
            tx_hash: H256::repeat_byte(1),
            tx_index_in_miniblock: 0,
            tx_initiator_address: Address::repeat_byte(2),
        };
        let events = vec![
            // Matches address, doesn't match topics
            VmEvent {
                location: (L1BatchNumber(1), 0),
                address: Address::repeat_byte(23),
                indexed_topics: vec![],
                value: b"1".to_vec(),
            },
            // Doesn't match address, matches topics
            VmEvent {
                location: (L1BatchNumber(1), 1),
                address: Address::zero(),
                indexed_topics: vec![H256::repeat_byte(42)],
                value: b"2".to_vec(),
            },
            // Doesn't match address or topics
            VmEvent {
                location: (L1BatchNumber(1), 2),
                address: Address::zero(),
                indexed_topics: vec![H256::repeat_byte(1), H256::repeat_byte(42)],
                value: b"3".to_vec(),
            },
            // Matches both address and topics
            VmEvent {
                location: (L1BatchNumber(1), 3),
                address: Address::repeat_byte(23),
                indexed_topics: vec![H256::repeat_byte(42), H256::repeat_byte(111)],
                value: b"4".to_vec(),
            },
        ];
        storage
            .events_dal()
            .save_events(
                MiniblockNumber(1),
                &[(tx_location, events.iter().collect())],
            )
            .await;
        (tx_location, events)
    }
}

#[async_trait]
impl WsTest for LogSubscriptions {
    async fn test(&self, client: &WsClient, pool: &ConnectionPool) {
        let params = rpc_params!["logs"];
        let mut all_logs_subscription = client
            .subscribe::<api::Log, _>("eth_subscribe", params, "eth_unsubscribe")
            .await
            .unwrap();
        let address_filter = PubSubFilter {
            address: Some(Address::repeat_byte(23).into()),
            topics: None,
        };
        let params = rpc_params!["logs", address_filter];
        let mut address_subscription = client
            .subscribe::<api::Log, _>("eth_subscribe", params, "eth_unsubscribe")
            .await
            .unwrap();
        let topic_filter = PubSubFilter {
            address: None,
            topics: Some(vec![Some(H256::repeat_byte(42).into())]),
        };
        let params = rpc_params!["logs", topic_filter];
        let mut topic_subscription = client
            .subscribe::<api::Log, _>("eth_subscribe", params, "eth_unsubscribe")
            .await
            .unwrap();

        // Wait a little until subscriptions are fully registered.
        tokio::time::sleep(Duration::from_millis(50)).await;

        let (tx_location, events) = Self::update_storage(pool).await;
        let events: Vec<_> = events.iter().collect();

        let all_logs = collect_logs(&mut all_logs_subscription, 4).await;
        for (i, log) in all_logs.iter().enumerate() {
            assert_eq!(log.transaction_index, Some(0.into()));
            assert_eq!(log.log_index, Some(i.into()));
            assert_eq!(log.transaction_hash, Some(tx_location.tx_hash));
            assert_eq!(log.block_number, Some(1.into()));
        }
        assert_logs_match(&all_logs, &events);

        let address_logs = collect_logs(&mut address_subscription, 2).await;
        assert_logs_match(&address_logs, &[events[0], events[3]]);

        let topic_logs = collect_logs(&mut topic_subscription, 2).await;
        assert_logs_match(&topic_logs, &[events[1], events[3]]);
    }
}

async fn collect_logs(sub: &mut Subscription<api::Log>, expected_count: usize) -> Vec<api::Log> {
    let mut logs = Vec::with_capacity(expected_count);
    for _ in 0..expected_count {
        let log = tokio::time::timeout(TEST_TIMEOUT, sub.next())
            .await
            .expect("Timed out waiting for new log")
            .expect("Logs subscription terminated")
            .unwrap();
        logs.push(log);
    }
    logs
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
struct BasicFilterChanges;

#[async_trait]
impl WsTest for BasicFilterChanges {
    async fn test(&self, client: &WsClient, pool: &ConnectionPool) {
        let block_filter_id = client.new_block_filter().await.unwrap();
        let tx_filter_id = client.new_pending_transaction_filter().await.unwrap();

        let (new_miniblock, new_tx_hash) = BasicSubscriptions::update_storage(pool).await;

        let block_filter_changes = client.get_filter_changes(block_filter_id).await.unwrap();
        assert_matches!(
            block_filter_changes,
            FilterChanges::Hashes(hashes) if hashes == [new_miniblock.hash]
        );
        let block_filter_changes = client.get_filter_changes(block_filter_id).await.unwrap();
        assert_matches!(block_filter_changes, FilterChanges::Hashes(hashes) if hashes.is_empty());

        let tx_filter_changes = client.get_filter_changes(tx_filter_id).await.unwrap();
        assert_matches!(
            tx_filter_changes,
            FilterChanges::Hashes(hashes) if hashes == [new_tx_hash]
        );
        let tx_filter_changes = client.get_filter_changes(tx_filter_id).await.unwrap();
        assert_matches!(tx_filter_changes, FilterChanges::Hashes(hashes) if hashes.is_empty());

        // Check uninstalling the filter.
        let removed = client.uninstall_filter(block_filter_id).await.unwrap();
        assert!(removed);
        let removed = client.uninstall_filter(block_filter_id).await.unwrap();
        assert!(!removed);

        let err = client
            .get_filter_changes(block_filter_id)
            .await
            .unwrap_err();
        assert_matches!(err, RpcError::Call(err) if err.code() == ErrorCode::InvalidParams.code());
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
    async fn test(&self, client: &WsClient, pool: &ConnectionPool) {
        let all_logs_filter_id = client.new_filter(Filter::default()).await.unwrap();
        let address_filter = Filter {
            address: Some(Address::repeat_byte(23).into()),
            ..Filter::default()
        };
        let address_filter_id = client.new_filter(address_filter).await.unwrap();
        let topics_filter = Filter {
            topics: Some(vec![Some(H256::repeat_byte(42).into())]),
            ..Filter::default()
        };
        let topics_filter_id = client.new_filter(topics_filter).await.unwrap();

        let (_, events) = LogSubscriptions::update_storage(pool).await;
        let events: Vec<_> = events.iter().collect();

        let all_logs = client.get_filter_changes(all_logs_filter_id).await.unwrap();
        let FilterChanges::Logs(all_logs) = all_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", all_logs);
        };
        assert_logs_match(&all_logs, &events);

        let address_logs = client.get_filter_changes(address_filter_id).await.unwrap();
        let FilterChanges::Logs(address_logs) = address_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", address_logs);
        };
        assert_logs_match(&address_logs, &[events[0], events[3]]);

        let topics_logs = client.get_filter_changes(topics_filter_id).await.unwrap();
        let FilterChanges::Logs(topics_logs) = topics_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", topics_logs);
        };
        assert_logs_match(&topics_logs, &[events[1], events[3]]);

        let new_all_logs = client.get_filter_changes(all_logs_filter_id).await.unwrap();
        let FilterChanges::Logs(new_all_logs) = new_all_logs else {
            panic!("Unexpected getFilterChanges output: {:?}", new_all_logs);
        };
        assert_eq!(new_all_logs, all_logs); // FIXME: is this expected?
    }
}

#[tokio::test]
async fn log_filter_changes() {
    test_ws_server(LogFilterChanges).await;
}
