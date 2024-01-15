use std::{
    num::NonZeroU32,
    sync::Arc,
    time::{Duration, Instant},
};

use assert_matches::assert_matches;
use async_trait::async_trait;
use jsonrpsee::{
    http_client::HttpClient,
    ws_client::{WsClient, WsClientBuilder},
};
use multivm::zk_evm_1_3_1::ethereum_types::H256;
use tokio::sync::{mpsc, watch};
use zksync_config::{
    configs::{
        api::Web3JsonRpcConfig,
        chain::{NetworkConfig, StateKeeperConfig},
    },
    ContractsConfig,
};
use zksync_dal::{transactions_dal::L2TxSubmissionResult, ConnectionPool, StorageProcessor};
use zksync_health_check::CheckHealth;
use zksync_state::PostgresStorageCaches;
use zksync_system_constants::{
    L1_GAS_PER_PUBDATA_BYTE, L2_ETH_TOKEN_ADDRESS, TRANSFER_EVENT_TOPIC,
};
use zksync_types::{
    api, api::ApiEthTransferEvents, block::MiniblockHeader, fee::TransactionExecutionMetrics,
    tx::IncludedTxLocation, Address, L1BatchNumber, MiniblockNumber, VmEvent,
};

use crate::{
    api_server::{
        tx_sender::TxSenderConfig,
        web3::{
            metrics::{ApiTransportLabel, SubscriptionType},
            pubsub::PubSubEvent,
            state::InternalApiConfig,
            ApiBuilder, ApiServerHandles, Namespace,
        },
    },
    genesis::{ensure_genesis_state, GenesisParams},
    l1_gas_price::L1GasPriceProvider,
    utils::testonly::{create_l2_transaction, create_miniblock},
};

pub(super) const TEST_TIMEOUT: Duration = Duration::from_secs(10);
pub(super) const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Mock [`L1GasPriceProvider`] that returns a constant value.
#[derive(Debug)]
pub(super) struct MockL1GasPriceProvider(u64);

impl L1GasPriceProvider for MockL1GasPriceProvider {
    fn estimate_effective_gas_price(&self) -> u64 {
        self.0
    }

    fn estimate_effective_pubdata_price(&self) -> u64 {
        self.0 * L1_GAS_PER_PUBDATA_BYTE as u64
    }
}

#[async_trait]
pub(super) trait HttpTest {
    async fn test(&self, client: &HttpClient, pool: &ConnectionPool) -> anyhow::Result<()>;

    fn api_eth_transfer_events(&self) -> ApiEthTransferEvents;
}

pub(super) async fn test_http_server(test: impl HttpTest) {
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
    let server_handles = spawn_http_server(
        &network_config,
        pool.clone(),
        stop_receiver,
        test.api_eth_transfer_events(),
    )
    .await;
    server_handles.wait_until_ready().await;

    let client = <HttpClient>::builder()
        .build(format!("http://{}/", server_handles.local_addr))
        .unwrap();
    test.test(&client, &pool).await.unwrap();

    stop_sender.send_replace(true);
    server_handles.shutdown().await;
}

#[async_trait]
pub(super) trait WsTest {
    async fn test(
        &self,
        client: &WsClient,
        pool: &ConnectionPool,
        pub_sub_events: mpsc::UnboundedReceiver<PubSubEvent>,
    ) -> anyhow::Result<()>;

    fn api_eth_transfer_events(&self) -> ApiEthTransferEvents;

    fn websocket_requests_per_minute_limit(&self) -> Option<NonZeroU32> {
        None
    }
}

pub(super) async fn test_ws_server(test: impl WsTest) {
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
        test.api_eth_transfer_events(),
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

impl ApiServerHandles {
    /// Waits until the server health check reports the ready state.
    pub(crate) async fn wait_until_ready(&self) {
        let started_at = Instant::now();
        loop {
            assert!(
                started_at.elapsed() <= TEST_TIMEOUT,
                "Timed out waiting for API server"
            );
            let health = self.health_check.check_health().await;
            if health.status().is_ready() {
                break;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    pub(crate) async fn shutdown(self) {
        let stop_server = async {
            for task in self.tasks {
                let task_result = task.await.unwrap_or_else(|err| {
                    if err.is_cancelled() {
                        Ok(())
                    } else {
                        panic!("Server panicked: {err:?}");
                    }
                });
                task_result.expect("Server task returned an error");
            }
        };
        tokio::time::timeout(TEST_TIMEOUT, stop_server)
            .await
            .expect(format!("panicking at {}", chrono::Utc::now()).as_str());
    }
}

pub(crate) async fn spawn_http_server(
    network_config: &NetworkConfig,
    pool: ConnectionPool,
    stop_receiver: watch::Receiver<bool>,
    api_eth_transfer_events: ApiEthTransferEvents,
) -> ApiServerHandles {
    spawn_server(
        ApiTransportLabel::Http,
        network_config,
        pool,
        stop_receiver,
        None,
        api_eth_transfer_events,
    )
    .await
    .0
}

pub(super) fn get_expected_events(
    events: Vec<&VmEvent>,
    api_eth_transfer_events: ApiEthTransferEvents,
) -> Vec<&VmEvent> {
    match api_eth_transfer_events {
        ApiEthTransferEvents::Enabled => events,
        ApiEthTransferEvents::Disabled => {
            // filter out all the ETH Transfer
            events
                .into_iter()
                .filter(|event| {
                    !(event.address == L2_ETH_TOKEN_ADDRESS
                        && !event.indexed_topics.is_empty()
                        && event.indexed_topics[0] == TRANSFER_EVENT_TOPIC)
                })
                .collect()
        }
    }
}

async fn spawn_ws_server(
    network_config: &NetworkConfig,
    pool: ConnectionPool,
    stop_receiver: watch::Receiver<bool>,
    websocket_requests_per_minute_limit: Option<NonZeroU32>,
    api_eth_transfer_events: ApiEthTransferEvents,
) -> (ApiServerHandles, mpsc::UnboundedReceiver<PubSubEvent>) {
    spawn_server(
        ApiTransportLabel::Ws,
        network_config,
        pool,
        stop_receiver,
        websocket_requests_per_minute_limit,
        api_eth_transfer_events,
    )
    .await
}

async fn spawn_server(
    transport: ApiTransportLabel,
    network_config: &NetworkConfig,
    pool: ConnectionPool,
    stop_receiver: watch::Receiver<bool>,
    websocket_requests_per_minute_limit: Option<NonZeroU32>,
    api_eth_transfer_events: ApiEthTransferEvents,
) -> (ApiServerHandles, mpsc::UnboundedReceiver<PubSubEvent>) {
    let contracts_config = ContractsConfig::for_tests();
    let mut web3_config = Web3JsonRpcConfig::for_tests();

    let api_eth_transfer_events = match api_eth_transfer_events {
        ApiEthTransferEvents::Enabled => zksync_config::configs::api::ApiEthTransferEvents::Enabled,
        ApiEthTransferEvents::Disabled => {
            zksync_config::configs::api::ApiEthTransferEvents::Disabled
        }
    };

    web3_config.api_eth_transfer_events = api_eth_transfer_events;
    let state_keeper_config = StateKeeperConfig::for_tests();
    let api_config = InternalApiConfig::new(network_config, &web3_config, &contracts_config);
    let tx_sender_config =
        TxSenderConfig::new(&state_keeper_config, &web3_config, api_config.l2_chain_id);

    let storage_caches = PostgresStorageCaches::new(1, 1);
    let gas_adjuster = Arc::new(MockL1GasPriceProvider(1));
    let (tx_sender, vm_barrier) = crate::build_tx_sender(
        &tx_sender_config,
        &web3_config,
        &state_keeper_config,
        pool.clone(),
        pool.clone(),
        gas_adjuster,
        storage_caches,
    )
    .await;
    let (pub_sub_events_sender, pub_sub_events_receiver) = mpsc::unbounded_channel();

    let server_builder = match transport {
        ApiTransportLabel::Http => ApiBuilder::jsonrpsee_backend(api_config, pool).http(0),
        ApiTransportLabel::Ws => {
            let mut builder = ApiBuilder::jsonrpsee_backend(api_config, pool)
                .ws(0)
                .with_subscriptions_limit(100);
            if let Some(websocket_requests_per_minute_limit) = websocket_requests_per_minute_limit {
                builder = builder
                    .with_websocket_requests_per_minute_limit(websocket_requests_per_minute_limit);
            }
            builder
        }
    };

    let mut namespaces = Namespace::DEFAULT.to_vec();
    namespaces.push(Namespace::Snapshots);

    let server_handles = server_builder
        .with_polling_interval(POLL_INTERVAL)
        .with_tx_sender(tx_sender, vm_barrier)
        .with_pub_sub_events(pub_sub_events_sender)
        .enable_api_namespaces(namespaces)
        .build(stop_receiver)
        .await
        .expect("Failed spawning JSON-RPC server");
    (server_handles, pub_sub_events_receiver)
}

#[allow(clippy::needless_pass_by_ref_mut)] // false positive
pub(super) async fn wait_for_subscription(
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
pub(super) async fn wait_for_notifier(
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

pub(super) fn assert_logs_match(actual_logs: &[api::Log], expected_logs: &[&VmEvent]) {
    assert_eq!(actual_logs.len(), expected_logs.len());
    for (actual_log, &expected_log) in actual_logs.iter().zip(expected_logs) {
        assert_eq!(actual_log.address, expected_log.address);
        assert_eq!(actual_log.topics, expected_log.indexed_topics);
        assert_eq!(actual_log.data.0, expected_log.value);
    }
}

pub(super) async fn store_miniblock(
    storage: &mut StorageProcessor<'_>,
) -> anyhow::Result<(MiniblockHeader, H256)> {
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

pub(super) async fn store_events(
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
        VmEvent {
            location: (L1BatchNumber(1), start_idx + 4),
            address: L2_ETH_TOKEN_ADDRESS,
            indexed_topics: vec![TRANSFER_EVENT_TOPIC],
            value: (start_idx + 4).to_le_bytes().to_vec(),
        },
        // ETH Transfer event with only topic matching
        VmEvent {
            location: (L1BatchNumber(1), start_idx + 5),
            address: Address::repeat_byte(12),
            indexed_topics: vec![TRANSFER_EVENT_TOPIC],
            value: (start_idx + 5).to_le_bytes().to_vec(),
        },
        // ETH Transfer event with only address matching
        VmEvent {
            location: (L1BatchNumber(1), start_idx + 6),
            address: L2_ETH_TOKEN_ADDRESS,
            indexed_topics: vec![H256::repeat_byte(25)],
            value: (start_idx + 6).to_le_bytes().to_vec(),
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
