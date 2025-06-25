//! Test utilities useful for writing unit tests outside of this crate.

use std::{net::Ipv4Addr, time::Instant};

use tokio::sync::watch;
use zksync_config::configs::{
    api::{Namespace, Web3JsonRpcConfig},
    chain::StateKeeperConfig,
    wallets::Wallets,
};
use zksync_dal::ConnectionPool;
use zksync_health_check::CheckHealth;
use zksync_node_fee_model::MockBatchFeeParamsProvider;
use zksync_state::PostgresStorageCaches;
use zksync_types::L2ChainId;
use zksync_vm_executor::oneshot::MockOneshotExecutor;

use super::*;
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
        pool.clone(),
        pool,
        batch_fee_model_input_provider,
        storage_caches,
    )
    .await
    .expect("failed building transaction sender");

    let tx_sender_inner = Arc::get_mut(&mut tx_sender.0).unwrap();
    tx_sender_inner.executor = tx_executor;
    tx_sender_inner.transaction_filter = Arc::new(()); // prevents "unexecutable transaction" errors
    (tx_sender, vm_barrier)
}

/// Handles to the initialized API server.
#[derive(Debug)]
pub struct ApiServerHandles {
    pub tasks: Vec<JoinHandle<anyhow::Result<()>>>,
    pub health_check: ReactiveHealthCheck,
    pub pub_sub_events: mpsc::UnboundedReceiver<PubSubEvent>,
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
            if matches!(health.status(), HealthStatus::Ready) {
                let health_details = health.details().unwrap();
                break serde_json::from_value(health_details["local_addr"].clone())
                    .expect("invalid `local_addr` in health details");
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
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
    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_method_tracer(mut self, tracer: Arc<MethodTracer>) -> Self {
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
        self.spawn_server(
            ApiTransport::Http((Ipv4Addr::LOCALHOST, 0).into()),
            None,
            stop_receiver,
        )
        .await
    }

    /// **Important.** Only a few of `web3_config` params are used!
    pub(crate) async fn spawn_server(
        self,
        transport: ApiTransport,
        web3_config: Option<&Web3JsonRpcConfig>,
        stop_receiver: watch::Receiver<bool>,
    ) -> ApiServerHandles {
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
        let (pub_sub_events_sender, pub_sub_events) = mpsc::unbounded_channel();

        let sealed_l2_block_handle = SealedL2BlockNumber::default();
        let bridge_addresses_handle =
            BridgeAddressesHandle::new(api_config.bridge_addresses.clone());

        let mut server_tasks = vec![];
        let pub_sub = transport.has_ws().then(|| {
            let mut pub_sub = EthSubscribe::new(POLL_INTERVAL);
            pub_sub.set_events_sender(pub_sub_events_sender);
            server_tasks.extend(pub_sub.spawn_notifiers(&pool, &stop_receiver));
            pub_sub
        });

        let mut server_builder = ApiBuilder::new(api_config, pool);
        server_builder.transport = Some(transport);
        if let Some(web3_config) = web3_config {
            server_builder = server_builder
                .with_websocket_requests_per_minute_limit(
                    web3_config.websocket_requests_per_minute_limit,
                )
                .enable_http_namespaces(web3_config.api_namespaces.clone())
                .enable_ws_namespaces(web3_config.ws_namespaces().clone());
            if let Some(timeout) = web3_config.request_timeout {
                server_builder = server_builder.with_request_timeout(timeout);
            }
        } else {
            let mut namespaces = HashSet::from(Namespace::DEFAULT);
            namespaces.extend([Namespace::Debug, Namespace::Snapshots, Namespace::Unstable]);
            server_builder = server_builder
                .enable_http_namespaces(namespaces.clone())
                .enable_ws_namespaces(namespaces);
        }

        let server_builder = server_builder
            .with_tx_sender(tx_sender)
            .with_vm_barrier(vm_barrier)
            .with_method_tracer(method_tracer)
            .with_sealed_l2_block_handle(sealed_l2_block_handle)
            .with_bridge_addresses_handle(bridge_addresses_handle);

        let server = server_builder.build().expect("Unable to build API server");
        let health_check = server.health_checks().pop().unwrap();
        server_tasks.push(tokio::spawn(server.run(pub_sub, stop_receiver)));
        ApiServerHandles {
            tasks: server_tasks,
            health_check,
            pub_sub_events,
        }
    }
}
