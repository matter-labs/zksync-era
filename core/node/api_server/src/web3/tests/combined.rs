//! Tests for combined HTTP / WS server.

use std::fmt;

use zksync_types::web3::BlockHeader;
use zksync_web3_decl::{
    client::WsClient,
    jsonrpsee::core::client::{Error, SubscriptionClientT},
};

use super::{ws::WsTest, *};
use crate::web3::metrics::SubscriptionType;

#[async_trait]
trait CombinedTest: TestInit {
    async fn test(
        &self,
        http_client: &DynClient<L2>,
        ws_client: &WsClient<L2>,
        pool: &ConnectionPool<Core>,
        pub_sub_events: &mut mpsc::UnboundedReceiver<PubSubEvent>,
    ) -> anyhow::Result<()>;
}

async fn test_combined_server(test: impl CombinedTest) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let (stop_sender, stop_receiver) = watch::channel(false);
    let transport = ApiTransport::HttpAndWs((Ipv4Addr::LOCALHOST, 0).into());
    let (local_addr, mut server_handles) =
        prepare_server(transport, &pool, &test, stop_receiver).await;

    let http_client = Client::http(format!("http://{local_addr}/").parse().unwrap())
        .unwrap()
        .build();
    let ws_client = Client::ws(format!("ws://{local_addr}").parse().unwrap())
        .await
        .unwrap()
        .build();
    test.test(
        &http_client,
        &ws_client,
        &pool,
        &mut server_handles.pub_sub_events,
    )
    .await
    .unwrap();

    stop_sender.send_replace(true);
    server_handles.shutdown().await;
}

fn assert_not_found<T: fmt::Debug>(result: Result<T, Error>) {
    assert_matches!(result, Err(Error::Call(e)) => {
        assert_eq!(e.code(), ErrorCode::MethodNotFound.code());
        assert_eq!(e.message(), ErrorCode::MethodNotFound.message());
    });
}

#[derive(Debug)]
struct CombinedServerBasicTest;

impl TestInit for CombinedServerBasicTest {}

#[async_trait]
impl CombinedTest for CombinedServerBasicTest {
    async fn test(
        &self,
        http_client: &DynClient<L2>,
        ws_client: &WsClient<L2>,
        pool: &ConnectionPool<Core>,
        pub_sub_events: &mut mpsc::UnboundedReceiver<PubSubEvent>,
    ) -> anyhow::Result<()> {
        // Just delegate to already defined basic HTTP / WS tests.
        HttpServerBasicsTest.test(http_client, pool).await?;
        ws::WsServerCanStartTest
            .test(ws_client, pool, pub_sub_events)
            .await?;
        ws::BasicSubscriptionsTest::default()
            .test(ws_client, pool, pub_sub_events)
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn combined_server_basics() {
    test_combined_server(CombinedServerBasicTest).await;
}

#[derive(Debug)]
struct NamespaceFilteringTest;

impl TestInit for NamespaceFilteringTest {
    fn web3_config(&self) -> Web3JsonRpcConfig {
        Web3JsonRpcConfig {
            api_namespaces: HashSet::from([Namespace::Eth]),
            ws_api_namespaces: Some(HashSet::from([Namespace::Zks, Namespace::Pubsub])),
            ..Web3JsonRpcConfig::for_tests()
        }
    }
}

#[async_trait]
impl CombinedTest for NamespaceFilteringTest {
    async fn test(
        &self,
        http_client: &DynClient<L2>,
        ws_client: &WsClient<L2>,
        _pool: &ConnectionPool<Core>,
        pub_sub_events: &mut mpsc::UnboundedReceiver<PubSubEvent>,
    ) -> anyhow::Result<()> {
        let block_number = http_client.get_block_number().await?;
        assert_eq!(block_number, U64::from(0));
        let l1_batch_number = ws_client.get_l1_batch_number().await?;
        assert_eq!(l1_batch_number, U64::from(0));

        assert_not_found(ws_client.get_block_number().await);
        assert_not_found(http_client.get_l1_batch_number().await);

        // The HTTP server must not support subscriptions.
        assert_not_found(
            http_client
                .request::<String, _>("eth_subscribe", rpc_params!["newHeads"])
                .await,
        );

        // Subscriptions must work for the WS server.
        let params = rpc_params!["newHeads"];
        let blocks_subscription = ws_client
            .subscribe::<BlockHeader, _>("eth_subscribe", params, "eth_unsubscribe")
            .await?;
        ws::wait_for_subscription(pub_sub_events, SubscriptionType::Blocks).await;

        blocks_subscription.unsubscribe().await?;
        Ok(())
    }
}

#[tokio::test]
async fn namespace_filtering() {
    test_combined_server(NamespaceFilteringTest).await;
}
