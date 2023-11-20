use assert_matches::assert_matches;
use tokio::sync::watch;

use std::{sync::Arc, time::Instant};

use zksync_config::configs::{
    api::Web3JsonRpcConfig,
    chain::{NetworkConfig, StateKeeperConfig},
    ContractsConfig,
};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{transactions_dal::L2TxSubmissionResult, ConnectionPool};
use zksync_health_check::CheckHealth;
use zksync_state::PostgresStorageCaches;
use zksync_types::{
    block::MiniblockHeader, fee::TransactionExecutionMetrics, L1BatchNumber, ProtocolVersionId,
    H256, U64,
};
use zksync_web3_decl::types::BlockHeader;
use zksync_web3_decl::{
    jsonrpsee::{
        core::client::SubscriptionClientT,
        http_client::HttpClient,
        rpc_params,
        ws_client::{WsClient, WsClientBuilder},
    },
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
};

use super::{metrics::ApiTransportLabel, *};
use crate::{
    api_server::tx_sender::TxSenderConfig,
    genesis::{ensure_genesis_state, GenesisParams},
    state_keeper::tests::create_l2_transaction,
};

const TEST_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Mock [`L1GasPriceProvider`] that returns a constant value.
struct MockL1GasPriceProvider(u64);

impl L1GasPriceProvider for MockL1GasPriceProvider {
    fn estimate_effective_gas_price(&self) -> u64 {
        self.0
    }
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
                task.await
                    .expect("Server panicked")
                    .expect("Server terminated with error");
            }
        };
        tokio::time::timeout(TEST_TIMEOUT, stop_server)
            .await
            .unwrap();
    }
}

pub(crate) async fn spawn_http_server(
    network_config: &NetworkConfig,
    pool: ConnectionPool,
    stop_receiver: watch::Receiver<bool>,
) -> ApiServerHandles {
    spawn_server(ApiTransportLabel::Http, network_config, pool, stop_receiver).await
}

async fn spawn_server(
    transport: ApiTransportLabel,
    network_config: &NetworkConfig,
    pool: ConnectionPool,
    stop_receiver: watch::Receiver<bool>,
) -> ApiServerHandles {
    let contracts_config = ContractsConfig::for_tests();
    let web3_config = Web3JsonRpcConfig::for_tests();
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

    let server_builder = match transport {
        ApiTransportLabel::Http => ApiBuilder::jsonrpsee_backend(api_config, pool).http(0),
        ApiTransportLabel::Ws => ApiBuilder::jsonrpc_backend(api_config, pool)
            .ws(0)
            .with_polling_interval(Duration::from_millis(50))
            .with_subscriptions_limit(100),
    };
    server_builder
        .with_threads(1)
        .with_tx_sender(tx_sender, vm_barrier)
        .enable_api_namespaces(Namespace::NON_DEBUG.to_vec())
        .build(stop_receiver)
        .await
        .expect("Failed spawning JSON-RPC server")
}

#[tokio::test]
async fn http_server_can_start() {
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
    let server_handles = spawn_http_server(&network_config, pool, stop_receiver).await;
    server_handles.wait_until_ready().await;

    test_http_server_methods(server_handles.local_addr).await;

    stop_sender.send_replace(true);
    server_handles.shutdown().await;
}

async fn test_http_server_methods(local_addr: SocketAddr) {
    let client = <HttpClient>::builder()
        .build(format!("http://{local_addr}/"))
        .unwrap();
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

#[tokio::test]
async fn ws_server_can_start() {
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

    let client = test_ws_server_basics(server_handles.local_addr).await;
    test_basic_subscriptions(&client, &pool).await;

    stop_sender.send_replace(true);
    server_handles.shutdown().await;
}

async fn test_ws_server_basics(local_addr: SocketAddr) -> WsClient {
    let client = WsClientBuilder::default()
        .build(format!("ws://{local_addr}"))
        .await
        .unwrap();
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

    client
}

async fn test_basic_subscriptions(client: &WsClient, pool: &ConnectionPool) {
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
    drop(storage);

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
