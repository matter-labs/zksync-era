//! Test utilities useful for writing unit tests outside of this crate.

use std::{pin::Pin, time::Instant};

use tokio::sync::watch;
use zksync_config::configs::{api::Web3JsonRpcConfig, chain::StateKeeperConfig, wallets::Wallets};
use zksync_dal::ConnectionPool;
use zksync_health_check::CheckHealth;
use zksync_node_fee_model::MockBatchFeeParamsProvider;
use zksync_state::PostgresStorageCaches;
use zksync_state_keeper::seal_criteria::NoopSealer;
use zksync_types::L2ChainId;
use zksync_vm_executor::oneshot::MockOneshotExecutor;

use super::{metrics::ApiTransportLabel, *};
use crate::{
    execution_sandbox::SandboxExecutor,
    tx_sender::{SandboxExecutorOptions, TxSenderConfig},
};

const TEST_TIMEOUT: Duration = Duration::from_secs(90);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

pub(crate) async fn create_test_tx_sender(
    pool: ConnectionPool<Core>,
    l2_chain_id: L2ChainId,
    tx_executor: SandboxExecutor,
) -> (TxSender, VmConcurrencyBarrier) {
    let web3_config = Web3JsonRpcConfig::for_tests();
    let state_keeper_config = StateKeeperConfig::for_tests();
    let wallets = Wallets::for_tests();
    let tx_sender_config = TxSenderConfig::new(
        &state_keeper_config,
        &web3_config,
        wallets.fee_account.unwrap().address(),
        l2_chain_id,
    );

    let storage_caches = PostgresStorageCaches::new(1, 1);
    let batch_fee_model_input_provider = Arc::<MockBatchFeeParamsProvider>::default();
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

    let tx_sender_inner = Arc::get_mut(&mut tx_sender.0).unwrap();
    tx_sender_inner.executor = tx_executor;
    tx_sender_inner.sealer = Arc::new(NoopSealer); // prevents "unexecutable transaction" errors
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

/// Builder for test server instances.
#[derive(Debug)]
pub struct TestServerBuilder {
    pool: ConnectionPool<Core>,
    api_config: InternalApiConfig,
    tx_executor: MockOneshotExecutor,
    executor_options: Option<SandboxExecutorOptions>,
    method_tracer: Arc<MethodTracer>,
}

impl TestServerBuilder {
    /// Creates a new builder.
    pub fn new(pool: ConnectionPool<Core>, api_config: InternalApiConfig) -> Self {
        Self {
            api_config,
            pool,
            tx_executor: MockOneshotExecutor::default(),
            executor_options: None,
            method_tracer: Arc::default(),
        }
    }

    /// Sets a transaction / call executor for this builder.
    #[must_use]
    pub fn with_tx_executor(mut self, tx_executor: MockOneshotExecutor) -> Self {
        self.tx_executor = tx_executor;
        self
    }

    /// Sets an RPC method tracer for this builder.
    #[must_use]
    pub fn with_method_tracer(mut self, tracer: Arc<MethodTracer>) -> Self {
        self.method_tracer = tracer;
        self
    }

    #[must_use]
    pub fn with_executor_options(mut self, options: SandboxExecutorOptions) -> Self {
        self.executor_options = Some(options);
        self
    }

    /// Builds an HTTP server.
    pub async fn build_http(self, stop_receiver: watch::Receiver<bool>) -> ApiServerHandles {
        self.spawn_server(ApiTransportLabel::Http, None, stop_receiver)
            .await
            .0
    }

    /// Builds a WS server.
    pub async fn build_ws(
        self,
        websocket_requests_per_minute_limit: Option<NonZeroU32>,
        stop_receiver: watch::Receiver<bool>,
    ) -> (ApiServerHandles, mpsc::UnboundedReceiver<PubSubEvent>) {
        self.spawn_server(
            ApiTransportLabel::Ws,
            websocket_requests_per_minute_limit,
            stop_receiver,
        )
        .await
    }

    async fn spawn_server(
        self,
        transport: ApiTransportLabel,
        websocket_requests_per_minute_limit: Option<NonZeroU32>,
        stop_receiver: watch::Receiver<bool>,
    ) -> (ApiServerHandles, mpsc::UnboundedReceiver<PubSubEvent>) {
        let Self {
            tx_executor,
            executor_options,
            pool,
            api_config,
            method_tracer,
        } = self;

        let tx_executor = if let Some(options) = executor_options {
            SandboxExecutor::custom_mock(tx_executor, options)
        } else {
            SandboxExecutor::mock(tx_executor).await
        };
        let (tx_sender, vm_barrier) =
            create_test_tx_sender(pool.clone(), api_config.l2_chain_id, tx_executor).await;
        let (pub_sub_events_sender, pub_sub_events_receiver) = mpsc::unbounded_channel();

        let mut namespaces = Namespace::DEFAULT.to_vec();
        namespaces.extend([Namespace::Debug, Namespace::Snapshots, Namespace::Unstable]);
        let sealed_l2_block_handle = SealedL2BlockNumber::default();
        let bridge_addresses_handle =
            BridgeAddressesHandle::new(api_config.bridge_addresses.clone());

        let server_builder = match transport {
            ApiTransportLabel::Http => ApiBuilder::jsonrpsee_backend(api_config, pool).http(0),
            ApiTransportLabel::Ws => {
                let mut builder = ApiBuilder::jsonrpsee_backend(api_config, pool)
                    .ws(0)
                    .with_subscriptions_limit(100);
                if let Some(websocket_requests_per_minute_limit) =
                    websocket_requests_per_minute_limit
                {
                    builder = builder.with_websocket_requests_per_minute_limit(
                        websocket_requests_per_minute_limit,
                    );
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
            .with_sealed_l2_block_handle(sealed_l2_block_handle)
            .with_bridge_addresses_handle(bridge_addresses_handle)
            .build()
            .expect("Unable to build API server")
            .run(stop_receiver)
            .await
            .expect("Failed spawning JSON-RPC server");
        (server_handles, pub_sub_events_receiver)
    }
}
