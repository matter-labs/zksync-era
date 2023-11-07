use tokio::sync::watch;

use std::{sync::Arc, time::Instant};

use zksync_config::configs::{
    api::Web3JsonRpcConfig,
    chain::{NetworkConfig, StateKeeperConfig},
    ContractsConfig,
};
use zksync_dal::ConnectionPool;
use zksync_health_check::CheckHealth;
use zksync_state::PostgresStorageCaches;
use zksync_types::{L1BatchNumber, U64};
use zksync_web3_decl::{
    jsonrpsee::http_client::HttpClient,
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
};

use super::*;
use crate::{
    api_server::tx_sender::TxSenderConfig,
    genesis::{ensure_genesis_state, GenesisParams},
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

    ApiBuilder::jsonrpsee_backend(api_config, pool)
        .http(0) // Assign random port
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
