//! WS-related tests.

use async_trait::async_trait;
use jsonrpsee::core::{client::ClientT, params::BatchRequestBuilder, ClientError};
use reqwest::StatusCode;
use tokio::sync::watch;
use zksync_config::configs::chain::NetworkConfig;
use zksync_dal::ConnectionPool;
use zksync_types::{api, Address, L1BatchNumber, H256, U64};
use zksync_web3_decl::{
    jsonrpsee::{
        core::client::{Subscription, SubscriptionClientT},
        rpc_params,
        ws_client::{WsClient, WsClientBuilder},
    },
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
    types::{BlockHeader, PubSubFilter},
};

use super::*;
use crate::api_server::web3::metrics::SubscriptionType;

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

    fn websocket_requests_per_minute_limit(&self) -> Option<NonZeroU32> {
        None
    }
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
    let (server_handles, pub_sub_events) = spawn_ws_server(
        &network_config,
        pool.clone(),
        stop_receiver,
        test.websocket_requests_per_minute_limit(),
    )
    .await;
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
struct WsServerCanStartTest;

#[async_trait]
impl WsTest for WsServerCanStartTest {
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
    test_ws_server(WsServerCanStartTest).await;
}

#[derive(Debug)]
struct BasicSubscriptionsTest;

#[async_trait]
impl WsTest for BasicSubscriptionsTest {
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

        let (new_miniblock, new_tx_hash) =
            store_miniblock(&mut pool.access_storage().await?).await?;

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
    test_ws_server(BasicSubscriptionsTest).await;
}

#[derive(Debug)]
struct LogSubscriptionsTest;

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
impl WsTest for LogSubscriptionsTest {
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
        let (tx_location, events) = store_events(&mut storage, 1, 0).await?;
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

#[tokio::test]
async fn log_subscriptions() {
    test_ws_server(LogSubscriptionsTest).await;
}

#[derive(Debug)]
struct LogSubscriptionsWithNewBlockTest;

#[async_trait]
impl WsTest for LogSubscriptionsWithNewBlockTest {
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
        let (_, events) = store_events(&mut storage, 1, 0).await?;
        drop(storage);
        let events: Vec<_> = events.iter().collect();

        let all_logs = collect_logs(&mut all_logs_subscription, 4).await?;
        assert_logs_match(&all_logs, &events);

        // Create a new block and wait for the pub-sub notifier to run.
        let mut storage = pool.access_storage().await?;
        let (_, new_events) = store_events(&mut storage, 2, 4).await?;
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
    test_ws_server(LogSubscriptionsWithNewBlockTest).await;
}

#[derive(Debug)]
struct LogSubscriptionsWithManyBlocksTest;

#[async_trait]
impl WsTest for LogSubscriptionsWithManyBlocksTest {
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
        let (_, events) = store_events(&mut transaction, 1, 0).await?;
        let events: Vec<_> = events.iter().collect();
        let (_, new_events) = store_events(&mut transaction, 2, 4).await?;
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
    test_ws_server(LogSubscriptionsWithManyBlocksTest).await;
}

#[derive(Debug)]
struct LogSubscriptionsWithDelayTest;

#[async_trait]
impl WsTest for LogSubscriptionsWithDelayTest {
    async fn test(
        &self,
        client: &WsClient,
        pool: &ConnectionPool,
        mut pub_sub_events: mpsc::UnboundedReceiver<PubSubEvent>,
    ) -> anyhow::Result<()> {
        // Store a miniblock w/o subscriptions being present.
        let mut storage = pool.access_storage().await?;
        store_events(&mut storage, 1, 0).await?;
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
        let (_, new_events) = store_events(&mut storage, 2, 4).await?;
        drop(storage);
        let new_events: Vec<_> = new_events.iter().collect();

        let all_logs = collect_logs(&mut all_logs_subscription, 4).await?;
        assert_logs_match(&all_logs, &new_events);
        let address_and_topic_logs = collect_logs(&mut address_and_topic_subscription, 1).await?;
        assert_logs_match(&address_and_topic_logs, &[new_events[3]]);

        // Check the behavior of remaining subscriptions if a subscription is dropped.
        all_logs_subscription.unsubscribe().await?;
        let mut storage = pool.access_storage().await?;
        let (_, new_events) = store_events(&mut storage, 3, 8).await?;
        drop(storage);

        let address_and_topic_logs = collect_logs(&mut address_and_topic_subscription, 1).await?;
        assert_logs_match(&address_and_topic_logs, &[&new_events[3]]);
        Ok(())
    }
}

#[tokio::test]
async fn log_subscriptions_with_delay() {
    test_ws_server(LogSubscriptionsWithDelayTest).await;
}

#[derive(Debug)]
struct RateLimitingTest;

#[async_trait]
impl WsTest for RateLimitingTest {
    async fn test(
        &self,
        client: &WsClient,
        _pool: &ConnectionPool,
        _pub_sub_events: mpsc::UnboundedReceiver<PubSubEvent>,
    ) -> anyhow::Result<()> {
        client.chain_id().await.unwrap();
        client.chain_id().await.unwrap();
        client.chain_id().await.unwrap();
        let expected_err = client.chain_id().await.unwrap_err();

        if let ClientError::Call(error) = expected_err {
            assert_eq!(error.code() as u16, StatusCode::TOO_MANY_REQUESTS.as_u16());
            assert_eq!(error.message(), "Too many requests");
            assert!(error.data().is_none());
        } else {
            panic!("Unexpected error returned: {expected_err}");
        }

        Ok(())
    }

    fn websocket_requests_per_minute_limit(&self) -> Option<NonZeroU32> {
        Some(NonZeroU32::new(3).unwrap())
    }
}

#[tokio::test]
async fn rate_limiting() {
    test_ws_server(RateLimitingTest).await;
}

#[derive(Debug)]
struct BatchGetsRateLimitedTest;

#[async_trait]
impl WsTest for BatchGetsRateLimitedTest {
    async fn test(
        &self,
        client: &WsClient,
        _pool: &ConnectionPool,
        _pub_sub_events: mpsc::UnboundedReceiver<PubSubEvent>,
    ) -> anyhow::Result<()> {
        client.chain_id().await.unwrap();
        client.chain_id().await.unwrap();

        let mut batch = BatchRequestBuilder::new();
        batch.insert("eth_chainId", rpc_params![]).unwrap();
        batch.insert("eth_chainId", rpc_params![]).unwrap();

        let mut expected_err = client
            .batch_request::<String>(batch)
            .await
            .unwrap()
            .into_ok()
            .unwrap_err();

        let error = expected_err.next().unwrap();

        assert_eq!(error.code() as u16, StatusCode::TOO_MANY_REQUESTS.as_u16());
        assert_eq!(error.message(), "Too many requests");
        assert!(error.data().is_none());

        Ok(())
    }

    fn websocket_requests_per_minute_limit(&self) -> Option<NonZeroU32> {
        Some(NonZeroU32::new(3).unwrap())
    }
}

#[tokio::test]
async fn batch_rate_limiting() {
    test_ws_server(BatchGetsRateLimitedTest).await;
}
