//! WS-related tests.

use std::num::NonZeroU32;

use async_trait::async_trait;
use jsonrpsee::core::{client::ClientT, params::BatchRequestBuilder, ClientError};
use reqwest::StatusCode;
use test_casing::test_casing;
use tokio::sync::mpsc;
use zksync_dal::ConnectionPool;
use zksync_types::{api, Address, L1BatchNumber, H256, U64};
use zksync_web3_decl::{
    jsonrpsee::{
        core::client::{Subscription, SubscriptionClientT},
        rpc_params,
        ws_client::WsClient,
    },
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
    types::{BlockHeader, PubSubFilter},
};

use super::*;
use crate::api_server::web3::{metrics::SubscriptionType, pubsub::PubSubEvent};

#[tokio::test]
async fn ws_server_can_start() {
    test_ws_server(WsServerCanStartTest).await;
}

#[tokio::test]
async fn basic_subscriptions() {
    test_ws_server(BasicSubscriptionsTest).await;
}

#[test_casing(2, [ApiEthTransferEvents::Disabled, ApiEthTransferEvents::Enabled])]
#[tokio::test]
async fn log_subscriptions(mode: ApiEthTransferEvents) {
    test_ws_server(LogSubscriptionsTest(mode)).await;
}

#[test_casing(2, [ApiEthTransferEvents::Disabled, ApiEthTransferEvents::Enabled])]
#[tokio::test]
async fn log_subscriptions_with_new_block(mode: ApiEthTransferEvents) {
    test_ws_server(LogSubscriptionsWithNewBlockTest(mode)).await;
}

#[test_casing(2, [ApiEthTransferEvents::Disabled, ApiEthTransferEvents::Enabled])]
#[tokio::test]
async fn log_subscriptions_with_many_new_blocks_at_once(mode: ApiEthTransferEvents) {
    test_ws_server(LogSubscriptionsWithManyBlocksTest(mode)).await;
}

#[test_casing(2, [ApiEthTransferEvents::Disabled, ApiEthTransferEvents::Enabled])]
#[tokio::test]
async fn log_subscriptions_with_delay(mode: ApiEthTransferEvents) {
    test_ws_server(LogSubscriptionsWithDelayTest(mode)).await;
}

#[tokio::test]
async fn rate_limiting() {
    test_ws_server(RateLimitingTest).await;
}

#[tokio::test]
async fn batch_rate_limiting() {
    test_ws_server(BatchGetsRateLimitedTest).await;
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

    fn api_eth_transfer_events(&self) -> ApiEthTransferEvents {
        ApiEthTransferEvents::Disabled
    }
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

    fn api_eth_transfer_events(&self) -> ApiEthTransferEvents {
        ApiEthTransferEvents::Disabled
    }
}

#[derive(Debug)]
struct LogSubscriptionsTest(ApiEthTransferEvents);

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

        let expected_count = if self.api_eth_transfer_events() == ApiEthTransferEvents::Disabled {
            6
        } else {
            7
        };

        drop(storage);

        let events: Vec<_> =
            get_expected_events(events.iter().collect(), self.api_eth_transfer_events());

        let all_logs = collect_logs(&mut all_logs_subscription, expected_count).await?;
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

    fn api_eth_transfer_events(&self) -> ApiEthTransferEvents {
        self.0
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

#[derive(Debug)]
struct LogSubscriptionsWithNewBlockTest(ApiEthTransferEvents);

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
        let events: Vec<_> =
            get_expected_events(events.iter().collect(), self.api_eth_transfer_events());

        let expected_count = if self.api_eth_transfer_events() == ApiEthTransferEvents::Disabled {
            6
        } else {
            7
        };
        let all_logs = collect_logs(&mut all_logs_subscription, expected_count).await?;
        assert_logs_match(&all_logs, &events);

        // Create a new block and wait for the pub-sub notifier to run.
        let mut storage = pool.access_storage().await?;
        let (_, new_events) = store_events(&mut storage, 2, 4).await?;
        drop(storage);
        let new_events: Vec<_> =
            get_expected_events(new_events.iter().collect(), self.api_eth_transfer_events());

        let all_new_logs = collect_logs(&mut all_logs_subscription, expected_count).await?;
        assert_logs_match(&all_new_logs, &new_events);

        let address_logs = collect_logs(&mut address_subscription, 4).await?;
        assert_logs_match(
            &address_logs,
            &[events[0], events[3], new_events[0], new_events[3]],
        );
        Ok(())
    }

    fn api_eth_transfer_events(&self) -> ApiEthTransferEvents {
        self.0
    }
}

#[derive(Debug)]
struct LogSubscriptionsWithManyBlocksTest(ApiEthTransferEvents);

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
        let events: Vec<_> =
            get_expected_events(events.iter().collect(), self.api_eth_transfer_events());
        let (_, new_events) = store_events(&mut transaction, 2, 4).await?;
        let new_events: Vec<_> =
            get_expected_events(new_events.iter().collect(), self.api_eth_transfer_events());
        transaction.commit().await?;
        drop(storage);

        let expected_count = if self.api_eth_transfer_events() == ApiEthTransferEvents::Disabled {
            6
        } else {
            7
        };

        let all_logs = collect_logs(&mut all_logs_subscription, expected_count).await?;
        assert_logs_match(&all_logs, &events);
        let all_new_logs = collect_logs(&mut all_logs_subscription, expected_count).await?;
        assert_logs_match(&all_new_logs, &new_events);

        let address_logs = collect_logs(&mut address_subscription, 4).await?;
        assert_logs_match(
            &address_logs,
            &[events[0], events[3], new_events[0], new_events[3]],
        );
        Ok(())
    }

    fn api_eth_transfer_events(&self) -> ApiEthTransferEvents {
        self.0
    }
}

#[derive(Debug)]
struct LogSubscriptionsWithDelayTest(ApiEthTransferEvents);

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
        let new_events: Vec<_> =
            get_expected_events(new_events.iter().collect(), self.api_eth_transfer_events());

        let expected_count = if self.api_eth_transfer_events() == ApiEthTransferEvents::Disabled {
            6
        } else {
            7
        };

        let all_logs = collect_logs(&mut all_logs_subscription, expected_count).await?;
        assert_logs_match(&all_logs, &new_events);
        let address_and_topic_logs = collect_logs(&mut address_and_topic_subscription, 1).await?;
        assert_logs_match(&address_and_topic_logs, &[new_events[3]]);

        // Check the behavior of remaining subscriptions if a subscription is dropped.
        all_logs_subscription.unsubscribe().await?;
        let mut storage = pool.access_storage().await?;
        let (_, new_events) = store_events(&mut storage, 3, 8).await?;
        let new_events =
            get_expected_events(new_events.iter().collect(), self.api_eth_transfer_events());

        drop(storage);

        let address_and_topic_logs = collect_logs(&mut address_and_topic_subscription, 1).await?;
        assert_logs_match(&address_and_topic_logs, &[&new_events[3]]);
        Ok(())
    }

    fn api_eth_transfer_events(&self) -> ApiEthTransferEvents {
        self.0
    }
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

    fn api_eth_transfer_events(&self) -> ApiEthTransferEvents {
        ApiEthTransferEvents::Disabled
    }

    fn websocket_requests_per_minute_limit(&self) -> Option<NonZeroU32> {
        Some(NonZeroU32::new(3).unwrap())
    }
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

    fn api_eth_transfer_events(&self) -> ApiEthTransferEvents {
        ApiEthTransferEvents::Disabled
    }

    fn websocket_requests_per_minute_limit(&self) -> Option<NonZeroU32> {
        Some(NonZeroU32::new(3).unwrap())
    }
}
