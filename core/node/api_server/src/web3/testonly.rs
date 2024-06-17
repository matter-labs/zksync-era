//! Test utilities useful for writing unit tests outside of this crate.

use std::{pin::Pin, time::Instant};

use tokio::sync::watch;
use zksync_config::configs::{api::Web3JsonRpcConfig, chain::StateKeeperConfig, wallets::Wallets};
use zksync_dal::ConnectionPool;
use zksync_health_check::CheckHealth;
use zksync_node_fee_model::MockBatchFeeParamsProvider;
use zksync_state::PostgresStorageCaches;
use zksync_types::L2ChainId;

use super::{metrics::ApiTransportLabel, *};
use crate::{
    execution_sandbox::{testonly::MockTransactionExecutor, TransactionExecutor},
    tx_sender::TxSenderConfig,
};

const TEST_TIMEOUT: Duration = Duration::from_secs(90);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

pub(crate) async fn create_test_tx_sender(
    pool: ConnectionPool<Core>,
    l2_chain_id: L2ChainId,
    tx_executor: TransactionExecutor,
) -> (TxSender, VmConcurrencyBarrier) {
    let web3_config = Web3JsonRpcConfig::for_tests();
    let state_keeper_config = StateKeeperConfig::for_tests();
    let wallets = Wallets::for_tests();
    let tx_sender_config = TxSenderConfig::new(
        &state_keeper_config,
        &web3_config,
        wallets.state_keeper.unwrap().fee_account.address(),
        l2_chain_id,
    );

    let storage_caches = PostgresStorageCaches::new(1, 1);
    let batch_fee_model_input_provider = Arc::new(MockBatchFeeParamsProvider::default());
    let (mut tx_sender, vm_barrier) = crate::tx_sender::build_tx_sender(
        &tx_sender_config,
        &web3_config,
        &state_keeper_config,
        pool.clone(),
        pool,
        batch_fee_model_input_provider,
        storage_caches,
    )
    .await
    .expect("failed building transaction sender");

    Arc::get_mut(&mut tx_sender.0).unwrap().executor = tx_executor;
    (tx_sender, vm_barrier)
}

impl ApiServerHandles {
    /// Waits until the server health check reports the ready state. Must be called once per server instance.
    pub async fn wait_until_ready(&mut self) -> SocketAddr {
        let started_at = Instant::now();
        loop {
            assert!(
                started_at.elapsed() <= TEST_TIMEOUT,
                "Timed out waiting for API server"
            );
            let health = self.health_check.check_health().await;
            if health.status().is_healthy() {
                break;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }

        let mut local_addr_future = Pin::new(&mut self.local_addr);
        local_addr_future
            .as_mut()
            .await
            .expect("API server panicked");
        local_addr_future.output_mut().copied().unwrap()
    }

    pub async fn shutdown(self) {
        let stop_server = async {
            for task in self.tasks {
                match task.await {
                    Ok(Ok(())) => { /* Task successfully completed */ }
                    Err(err) if err.is_cancelled() => {
                        // Task was canceled since the server runtime which runs the task was dropped.
                        // This is fine.
                    }
                    Err(err) => panic!("Server task panicked: {err:?}"),
                    Ok(Err(err)) => panic!("Server task failed: {err:?}"),
                }
            }
        };
        tokio::time::timeout(TEST_TIMEOUT, stop_server)
            .await
            .unwrap_or_else(|_| panic!("panicking at {}", chrono::Utc::now()));
    }
}

pub async fn spawn_http_server(
    api_config: InternalApiConfig,
    pool: ConnectionPool<Core>,
    tx_executor: MockTransactionExecutor,
    method_tracer: Arc<MethodTracer>,
    stop_receiver: watch::Receiver<bool>,
) -> ApiServerHandles {
    spawn_server(
        ApiTransportLabel::Http,
        api_config,
        pool,
        None,
        tx_executor,
        method_tracer,
        stop_receiver,
    )
    .await
    .0
}

pub async fn spawn_ws_server(
    api_config: InternalApiConfig,
    pool: ConnectionPool<Core>,
    stop_receiver: watch::Receiver<bool>,
    websocket_requests_per_minute_limit: Option<NonZeroU32>,
) -> (ApiServerHandles, mpsc::UnboundedReceiver<PubSubEvent>) {
    spawn_server(
        ApiTransportLabel::Ws,
        api_config,
        pool,
        websocket_requests_per_minute_limit,
        MockTransactionExecutor::default(),
        Arc::default(),
        stop_receiver,
    )
    .await
}

async fn spawn_server(
    transport: ApiTransportLabel,
    api_config: InternalApiConfig,
    pool: ConnectionPool<Core>,
    websocket_requests_per_minute_limit: Option<NonZeroU32>,
    tx_executor: MockTransactionExecutor,
    method_tracer: Arc<MethodTracer>,
    stop_receiver: watch::Receiver<bool>,
) -> (ApiServerHandles, mpsc::UnboundedReceiver<PubSubEvent>) {
    let (tx_sender, vm_barrier) =
        create_test_tx_sender(pool.clone(), api_config.l2_chain_id, tx_executor.into()).await;
    let (pub_sub_events_sender, pub_sub_events_receiver) = mpsc::unbounded_channel();

    let mut namespaces = Namespace::DEFAULT.to_vec();
    namespaces.extend([Namespace::Debug, Namespace::Snapshots]);

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
    let server_handles = server_builder
        .with_polling_interval(POLL_INTERVAL)
        .with_tx_sender(tx_sender)
        .with_vm_barrier(vm_barrier)
        .with_pub_sub_events(pub_sub_events_sender)
        .with_method_tracer(method_tracer)
        .enable_api_namespaces(namespaces)
        .build()
        .expect("Unable to build API server")
        .run(stop_receiver)
        .await
        .expect("Failed spawning JSON-RPC server");
    (server_handles, pub_sub_events_receiver)
}
