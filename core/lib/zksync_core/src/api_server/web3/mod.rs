use std::{collections::HashSet, net::SocketAddr, num::NonZeroU32, sync::Arc, time::Duration};

use anyhow::Context as _;
use chrono::NaiveDateTime;
use futures::future;
use serde::Deserialize;
use tokio::{
    sync::{mpsc, oneshot, watch, Mutex},
    task::JoinHandle,
};
use tower_http::{cors::CorsLayer, metrics::InFlightRequestsLayer};
use zksync_dal::{ConnectionPool, Core};
use zksync_health_check::{HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_state::MempoolCache;
use zksync_types::MiniblockNumber;
use zksync_web3_decl::{
    jsonrpsee::{
        server::{BatchRequestConfig, RpcServiceBuilder, ServerBuilder},
        RpcModule,
    },
    namespaces::{
        DebugNamespaceServer, EnNamespaceServer, EthNamespaceServer, EthPubSubServer,
        NetNamespaceServer, SnapshotsNamespaceServer, Web3NamespaceServer, ZksNamespaceServer,
    },
    types::Filter,
};

use self::{
    backend_jsonrpsee::{
        LimitMiddleware, MetadataMiddleware, MethodTracer, ShutdownMiddleware, TrafficTracker,
    },
    metrics::API_METRICS,
    namespaces::{
        DebugNamespace, EnNamespace, EthNamespace, NetNamespace, SnapshotsNamespace, Web3Namespace,
        ZksNamespace,
    },
    pubsub::{EthSubscribe, EthSubscriptionIdProvider, PubSubEvent},
    state::{Filters, InternalApiConfig, RpcState, SealedMiniblockNumber},
};
use crate::{
    api_server::{
        execution_sandbox::{BlockStartInfo, VmConcurrencyBarrier},
        tree::TreeApiClient,
        tx_sender::TxSender,
    },
    sync_layer::SyncState,
    utils::wait_for_l1_batch,
};

pub mod backend_jsonrpsee;
pub(super) mod metrics;
pub mod namespaces;
mod pubsub;
pub mod state;
#[cfg(test)]
pub(crate) mod tests;

/// Timeout for graceful shutdown logic within API servers.
const GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// Interval to wait for the traffic to be stopped to the API server (e.g., by a load balancer) before
/// the server will cease processing any further traffic. If this interval is exceeded, the server will start
/// shutting down anyway.
const NO_REQUESTS_WAIT_TIMEOUT: Duration = Duration::from_secs(30);
/// Time interval with no requests sent to the API server to declare that traffic to the server is ceased,
/// and start gracefully shutting down the server.
const SHUTDOWN_INTERVAL_WITHOUT_REQUESTS: Duration = Duration::from_millis(500);

/// Represents all kinds of `Filter`.
#[derive(Debug, Clone)]
pub(crate) enum TypedFilter {
    // Events from some block with additional filters
    Events(Filter, MiniblockNumber),
    // Blocks from some block
    Blocks(MiniblockNumber),
    // Pending transactions from some timestamp
    PendingTransactions(NaiveDateTime),
}

#[derive(Debug, Clone, Copy)]
enum ApiTransport {
    WebSocket(SocketAddr),
    Http(SocketAddr),
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Namespace {
    Eth,
    Net,
    Web3,
    Debug,
    Zks,
    En,
    Pubsub,
    Snapshots,
}

impl Namespace {
    pub const DEFAULT: &'static [Self] = &[
        Self::Eth,
        Self::Net,
        Self::Web3,
        Self::Zks,
        Self::En,
        Self::Pubsub,
    ];
}

/// Handles to the initialized API server.
#[derive(Debug)]
pub struct ApiServerHandles {
    pub tasks: Vec<JoinHandle<anyhow::Result<()>>>,
    pub health_check: ReactiveHealthCheck,
    #[allow(unused)] // only used in tests
    pub(crate) local_addr: future::TryMaybeDone<oneshot::Receiver<SocketAddr>>,
}

/// Optional part of the API server parameters.
#[derive(Debug, Default)]
struct OptionalApiParams {
    vm_barrier: Option<VmConcurrencyBarrier>,
    sync_state: Option<SyncState>,
    filters_limit: Option<usize>,
    subscriptions_limit: Option<usize>,
    batch_request_size_limit: Option<usize>,
    response_body_size_limit: Option<usize>,
    websocket_requests_per_minute_limit: Option<NonZeroU32>,
    tree_api: Option<Arc<dyn TreeApiClient>>,
    pub_sub_events_sender: Option<mpsc::UnboundedSender<PubSubEvent>>,
}

/// Structure capable of spawning a configured Web3 API server along with all the required
/// maintenance tasks.
#[derive(Debug)]
pub struct ApiServer {
    pool: ConnectionPool<Core>,
    updaters_pool: ConnectionPool<Core>,
    health_updater: Arc<HealthUpdater>,
    config: InternalApiConfig,
    transport: ApiTransport,
    tx_sender: TxSender,
    polling_interval: Duration,
    namespaces: Vec<Namespace>,
    method_tracer: Arc<MethodTracer>,
    optional: OptionalApiParams,
}

#[derive(Debug)]
pub struct ApiBuilder {
    pool: ConnectionPool<Core>,
    updaters_pool: ConnectionPool<Core>,
    config: InternalApiConfig,
    polling_interval: Duration,
    // Mandatory params that must be set using builder methods.
    transport: Option<ApiTransport>,
    tx_sender: Option<TxSender>,
    // Optional params that may or may not be set using builder methods. We treat `namespaces`
    // specially because we want to output a warning if they are not set.
    namespaces: Option<Vec<Namespace>>,
    method_tracer: Arc<MethodTracer>,
    optional: OptionalApiParams,
}

impl ApiBuilder {
    const DEFAULT_POLLING_INTERVAL: Duration = Duration::from_millis(200);

    pub fn jsonrpsee_backend(config: InternalApiConfig, pool: ConnectionPool<Core>) -> Self {
        Self {
            updaters_pool: pool.clone(),
            pool,
            config,
            polling_interval: Self::DEFAULT_POLLING_INTERVAL,
            transport: None,
            tx_sender: None,
            namespaces: None,
            method_tracer: Arc::new(MethodTracer::default()),
            optional: OptionalApiParams::default(),
        }
    }

    pub fn ws(mut self, port: u16) -> Self {
        self.transport = Some(ApiTransport::WebSocket(([0, 0, 0, 0], port).into()));
        self
    }

    pub fn http(mut self, port: u16) -> Self {
        self.transport = Some(ApiTransport::Http(([0, 0, 0, 0], port).into()));
        self
    }

    /// Configures a dedicated DB pool to be used for updating different information,
    /// such as last mined block number or account nonces. This pool is used to execute
    /// in a background task. If not called, the main pool will be used. If the API server is under high load,
    /// it may make sense to supply a single-connection pool to reduce pool contention with the API methods.
    pub fn with_updaters_pool(mut self, pool: ConnectionPool<Core>) -> Self {
        self.updaters_pool = pool;
        self
    }

    pub fn with_tx_sender(mut self, tx_sender: TxSender) -> Self {
        self.tx_sender = Some(tx_sender);
        self
    }

    pub fn with_vm_barrier(mut self, vm_barrier: VmConcurrencyBarrier) -> Self {
        self.optional.vm_barrier = Some(vm_barrier);
        self
    }

    pub fn with_filter_limit(mut self, filters_limit: usize) -> Self {
        self.optional.filters_limit = Some(filters_limit);
        self
    }

    pub fn with_subscriptions_limit(mut self, subscriptions_limit: usize) -> Self {
        self.optional.subscriptions_limit = Some(subscriptions_limit);
        self
    }

    pub fn with_batch_request_size_limit(mut self, batch_request_size_limit: usize) -> Self {
        self.optional.batch_request_size_limit = Some(batch_request_size_limit);
        self
    }

    pub fn with_response_body_size_limit(mut self, response_body_size_limit: usize) -> Self {
        self.optional.response_body_size_limit = Some(response_body_size_limit);
        self
    }

    pub fn with_websocket_requests_per_minute_limit(
        mut self,
        websocket_requests_per_minute_limit: NonZeroU32,
    ) -> Self {
        self.optional.websocket_requests_per_minute_limit =
            Some(websocket_requests_per_minute_limit);
        self
    }

    pub fn with_sync_state(mut self, sync_state: SyncState) -> Self {
        self.optional.sync_state = Some(sync_state);
        self
    }

    pub fn with_polling_interval(mut self, polling_interval: Duration) -> Self {
        self.polling_interval = polling_interval;
        self
    }

    pub fn enable_api_namespaces(mut self, namespaces: Vec<Namespace>) -> Self {
        self.namespaces = Some(namespaces);
        self
    }

    pub fn with_tree_api(mut self, tree_api: Arc<dyn TreeApiClient>) -> Self {
        tracing::info!("Using tree API client: {tree_api:?}");
        self.optional.tree_api = Some(tree_api);
        self
    }

    #[cfg(test)]
    fn with_pub_sub_events(mut self, sender: mpsc::UnboundedSender<PubSubEvent>) -> Self {
        self.optional.pub_sub_events_sender = Some(sender);
        self
    }

    #[cfg(test)]
    fn with_method_tracer(mut self, method_tracer: Arc<MethodTracer>) -> Self {
        self.method_tracer = method_tracer;
        self
    }
}

impl ApiBuilder {
    pub fn build(self) -> anyhow::Result<ApiServer> {
        let transport = self.transport.context("API transport not set")?;
        let health_check_name = match &transport {
            ApiTransport::Http(_) => "http_api",
            ApiTransport::WebSocket(_) => "ws_api",
        };
        let (_, health_updater) = ReactiveHealthCheck::new(health_check_name);

        Ok(ApiServer {
            pool: self.pool,
            health_updater: Arc::new(health_updater),
            updaters_pool: self.updaters_pool,
            config: self.config,
            transport,
            tx_sender: self.tx_sender.context("Transaction sender not set")?,
            polling_interval: self.polling_interval,
            namespaces: self.namespaces.unwrap_or_else(|| {
                tracing::warn!(
                    "debug_ and snapshots_ API namespace will be disabled by default in ApiBuilder"
                );
                Namespace::DEFAULT.to_vec()
            }),
            method_tracer: self.method_tracer,
            optional: self.optional,
        })
    }
}

impl ApiServer {
    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.health_updater.subscribe()
    }

    async fn build_rpc_state(
        self,
        last_sealed_miniblock: SealedMiniblockNumber,
        mempool_cache: MempoolCache,
    ) -> anyhow::Result<RpcState> {
        let mut storage = self.updaters_pool.connection_tagged("api").await?;
        let start_info = BlockStartInfo::new(&mut storage).await?;
        drop(storage);

        let installed_filters = if self.config.filters_disabled {
            None
        } else {
            Some(Arc::new(Mutex::new(Filters::new(
                self.optional.filters_limit,
            ))))
        };

        Ok(RpcState {
            current_method: self.method_tracer,
            installed_filters,
            connection_pool: self.pool,
            tx_sender: self.tx_sender,
            sync_state: self.optional.sync_state,
            api_config: self.config,
            start_info,
            mempool_cache,
            last_sealed_miniblock,
            tree_api: self.optional.tree_api,
        })
    }

    async fn build_rpc_module(
        self,
        pub_sub: Option<EthSubscribe>,
        last_sealed_miniblock: SealedMiniblockNumber,
        mempool_cache: MempoolCache,
    ) -> anyhow::Result<RpcModule<()>> {
        let namespaces = self.namespaces.clone();
        let zksync_network_id = self.config.l2_chain_id;
        let rpc_state = self
            .build_rpc_state(last_sealed_miniblock, mempool_cache)
            .await?;

        // Collect all the methods into a single RPC module.
        let mut rpc = RpcModule::new(());
        if let Some(pub_sub) = pub_sub {
            rpc.merge(pub_sub.into_rpc())
                .expect("Can't merge eth pubsub namespace");
        }

        if namespaces.contains(&Namespace::Eth) {
            rpc.merge(EthNamespace::new(rpc_state.clone()).into_rpc())
                .expect("Can't merge eth namespace");
        }
        if namespaces.contains(&Namespace::Net) {
            rpc.merge(NetNamespace::new(zksync_network_id).into_rpc())
                .expect("Can't merge net namespace");
        }
        if namespaces.contains(&Namespace::Web3) {
            rpc.merge(Web3Namespace.into_rpc())
                .expect("Can't merge web3 namespace");
        }
        if namespaces.contains(&Namespace::Zks) {
            rpc.merge(ZksNamespace::new(rpc_state.clone()).into_rpc())
                .expect("Can't merge zks namespace");
        }
        if namespaces.contains(&Namespace::En) {
            rpc.merge(EnNamespace::new(rpc_state.clone()).into_rpc())
                .expect("Can't merge en namespace");
        }
        if namespaces.contains(&Namespace::Debug) {
            rpc.merge(DebugNamespace::new(rpc_state.clone()).await.into_rpc())
                .expect("Can't merge debug namespace");
        }
        if namespaces.contains(&Namespace::Snapshots) {
            rpc.merge(SnapshotsNamespace::new(rpc_state).into_rpc())
                .expect("Can't merge snapshots namespace");
        }
        Ok(rpc)
    }

    pub async fn run(
        self,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<ApiServerHandles> {
        if self.config.filters_disabled {
            if self.optional.filters_limit.is_some() {
                tracing::warn!(
                    "Filters limit is not supported when filters are disabled, ignoring"
                );
            }
        } else if self.optional.filters_limit.is_none() {
            tracing::warn!("Filters limit is not set - unlimited filters are allowed");
        }

        if self.namespaces.contains(&Namespace::Pubsub)
            && matches!(&self.transport, ApiTransport::Http(_))
        {
            tracing::debug!("pubsub API is not supported for HTTP transport, ignoring");
        }

        match (&self.transport, self.optional.subscriptions_limit) {
            (ApiTransport::WebSocket(_), None) => {
                tracing::warn!(
                    "`subscriptions_limit` is not set - unlimited subscriptions are allowed"
                );
            }
            (ApiTransport::Http(_), Some(_)) => {
                tracing::warn!(
                    "`subscriptions_limit` is ignored for HTTP transport, use WebSocket instead"
                );
            }
            _ => {}
        }

        self.build_jsonrpsee(stop_receiver).await
    }

    async fn wait_for_vm(vm_barrier: VmConcurrencyBarrier, transport: &str) {
        let wait_for_vm =
            tokio::time::timeout(GRACEFUL_SHUTDOWN_TIMEOUT, vm_barrier.wait_until_stopped());
        if wait_for_vm.await.is_err() {
            tracing::warn!(
                "VM execution on {transport} JSON-RPC server didn't stop after {GRACEFUL_SHUTDOWN_TIMEOUT:?}; \
                 forcing shutdown anyway"
            );
        } else {
            tracing::info!("VM execution on {transport} JSON-RPC server stopped");
        }
    }

    async fn build_jsonrpsee(
        self,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<ApiServerHandles> {
        // Chosen to be significantly smaller than the interval between miniblocks, but larger than
        // the latency of getting the latest sealed miniblock number from Postgres. If the API server
        // processes enough requests, information about the latest sealed miniblock will be updated
        // by reporting block difference metrics, so the actual update lag would be much smaller than this value.
        const SEALED_MINIBLOCK_UPDATE_INTERVAL: Duration = Duration::from_millis(25);

        let transport = self.transport;

        let (last_sealed_miniblock, sealed_miniblock_update_task) = SealedMiniblockNumber::new(
            self.updaters_pool.clone(),
            SEALED_MINIBLOCK_UPDATE_INTERVAL,
            stop_receiver.clone(),
        );

        let mut tasks = vec![tokio::spawn(sealed_miniblock_update_task)];

        let (mempool_cache, mempool_cache_update_task) = MempoolCache::new(
            self.updaters_pool.clone(),
            self.config.mempool_cache_update_interval,
            self.config.mempool_cache_size,
            stop_receiver.clone(),
        );

        tasks.push(tokio::spawn(mempool_cache_update_task));

        let pub_sub = if matches!(transport, ApiTransport::WebSocket(_))
            && self.namespaces.contains(&Namespace::Pubsub)
        {
            let mut pub_sub = EthSubscribe::new();
            if let Some(sender) = &self.optional.pub_sub_events_sender {
                pub_sub.set_events_sender(sender.clone());
            }

            tasks.extend(pub_sub.spawn_notifiers(
                self.pool.clone(),
                self.polling_interval,
                stop_receiver.clone(),
            ));
            Some(pub_sub)
        } else {
            None
        };

        // TODO (QIT-26): We still expose `health_check` in `ApiServerHandles` for the old code. After we switch to the
        // framework it'll no longer be needed.
        let health_check = self.health_updater.subscribe();
        let (local_addr_sender, local_addr) = oneshot::channel();
        let server_task = tokio::spawn(self.run_jsonrpsee_server(
            stop_receiver,
            pub_sub,
            mempool_cache,
            last_sealed_miniblock,
            local_addr_sender,
        ));

        tasks.push(server_task);
        Ok(ApiServerHandles {
            health_check,
            tasks,
            local_addr: future::try_maybe_done(local_addr),
        })
    }

    async fn run_jsonrpsee_server(
        self,
        mut stop_receiver: watch::Receiver<bool>,
        pub_sub: Option<EthSubscribe>,
        mempool_cache: MempoolCache,
        last_sealed_miniblock: SealedMiniblockNumber,
        local_addr_sender: oneshot::Sender<SocketAddr>,
    ) -> anyhow::Result<()> {
        let transport = self.transport;
        let (transport_str, is_http, addr) = match transport {
            ApiTransport::Http(addr) => ("HTTP", true, addr),
            ApiTransport::WebSocket(addr) => ("WS", false, addr),
        };
        let transport_label = (&transport).into();

        tracing::info!(
            "Waiting for at least one L1 batch in Postgres to start {transport_str} API server"
        );
        // Starting the server before L1 batches are present in Postgres can lead to some invariants the server logic
        // implicitly assumes not being upheld. The only case when we'll actually wait here is immediately after snapshot recovery.
        let earliest_l1_batch_number =
            wait_for_l1_batch(&self.pool, self.polling_interval, &mut stop_receiver)
                .await
                .context("error while waiting for L1 batch in Postgres")?;

        if let Some(number) = earliest_l1_batch_number {
            tracing::info!("Successfully waited for at least one L1 batch in Postgres; the earliest one is #{number}");
        } else {
            tracing::info!("Received shutdown signal before {transport_str} API server is started; shutting down");
            return Ok(());
        }

        let batch_request_config = self
            .optional
            .batch_request_size_limit
            .map_or(BatchRequestConfig::Unlimited, |limit| {
                BatchRequestConfig::Limit(limit as u32)
            });
        let response_body_size_limit = self
            .optional
            .response_body_size_limit
            .map_or(u32::MAX, |limit| limit as u32);
        let websocket_requests_per_minute_limit = self.optional.websocket_requests_per_minute_limit;
        let subscriptions_limit = self.optional.subscriptions_limit;
        let vm_barrier = self.optional.vm_barrier.clone();
        let health_updater = self.health_updater.clone();
        let method_tracer = self.method_tracer.clone();

        let rpc = self
            .build_rpc_module(pub_sub, last_sealed_miniblock, mempool_cache)
            .await?;
        let registered_method_names = Arc::new(rpc.method_names().collect::<HashSet<_>>());
        tracing::debug!(
            "Built RPC module for {transport_str} server with {} methods: {registered_method_names:?}",
            registered_method_names.len()
        );

        // Setup CORS.
        let cors = is_http.then(|| {
            CorsLayer::new()
                // Allow `POST` when accessing the resource
                .allow_methods([reqwest::Method::POST])
                // Allow requests from any origin
                .allow_origin(tower_http::cors::Any)
                .allow_headers([reqwest::header::CONTENT_TYPE])
        });
        // Setup metrics for the number of in-flight requests.
        let (in_flight_requests, counter) = InFlightRequestsLayer::pair();
        tokio::spawn(
            counter.run_emitter(Duration::from_millis(100), move |count| {
                API_METRICS.web3_in_flight_requests[&transport_label].observe(count);
                future::ready(())
            }),
        );
        // Assemble server middleware.
        let middleware = tower::ServiceBuilder::new()
            .layer(in_flight_requests)
            .option_layer(cors);

        // Settings shared by HTTP and WS servers.
        let max_connections = !is_http
            .then_some(subscriptions_limit)
            .flatten()
            .unwrap_or(5_000);

        let traffic_tracker = TrafficTracker::default();
        let traffic_tracker_for_middleware = traffic_tracker.clone();
        let rpc_middleware = RpcServiceBuilder::new()
            .layer_fn(move |svc| {
                ShutdownMiddleware::new(svc, traffic_tracker_for_middleware.clone())
            })
            .layer_fn(move |svc| {
                MetadataMiddleware::new(svc, registered_method_names.clone(), method_tracer.clone())
            })
            .option_layer((!is_http).then(|| {
                tower::layer::layer_fn(move |svc| {
                    LimitMiddleware::new(svc, websocket_requests_per_minute_limit)
                })
            }));

        let server_builder = ServerBuilder::default()
            .max_connections(max_connections as u32)
            .set_http_middleware(middleware)
            .max_response_body_size(response_body_size_limit)
            .set_batch_request_config(batch_request_config)
            .set_rpc_middleware(rpc_middleware);

        let (local_addr, server_handle) = if is_http {
            // HTTP-specific settings
            let server = server_builder
                .http_only()
                .build(addr)
                .await
                .context("Failed building HTTP JSON-RPC server")?;
            (server.local_addr(), server.start(rpc))
        } else {
            // WS-specific settings
            let server = server_builder
                .set_id_provider(EthSubscriptionIdProvider)
                .build(addr)
                .await
                .context("Failed building WS JSON-RPC server")?;
            (server.local_addr(), server.start(rpc))
        };
        let local_addr = local_addr.with_context(|| {
            format!("Failed getting local address for {transport_str} JSON-RPC server")
        })?;
        tracing::info!("Initialized {transport_str} API on {local_addr:?}");
        local_addr_sender.send(local_addr).ok();
        health_updater.update(HealthStatus::Ready.into());

        // We want to be able to immediately stop the server task if the server stops on its own for whatever reason.
        // Hence, we monitor `stop_receiver` on a separate Tokio task.
        let close_handle = server_handle.clone();
        let closing_vm_barrier = vm_barrier.clone();
        // We use `Weak` reference to the health updater in order to not prevent its drop if the server stops on its own.
        // TODO (QIT-26): While `Arc<HealthUpdater>` is stored in `self`, we rely on the fact that `self` is consumed and
        // dropped by `self.build_rpc_module` above, so we should still have just one strong reference.
        let closing_health_updater = Arc::downgrade(&health_updater);
        tokio::spawn(async move {
            if stop_receiver.changed().await.is_err() {
                tracing::warn!(
                    "Stop signal sender for {transport_str} JSON-RPC server was dropped \
                     without sending a signal"
                );
            }
            if let Some(health_updater) = closing_health_updater.upgrade() {
                health_updater.update(HealthStatus::ShuttingDown.into());
            }
            tracing::info!(
                "Stop signal received, {transport_str} JSON-RPC server is shutting down"
            );

            // Wait some time until the traffic to the server stops. This may be necessary if the API server
            // is behind a load balancer which is not immediately aware of API server termination. In this case,
            // the load balancer will continue directing traffic to the server for some time until it reads
            // the server health (which *is* changed to "shutting down" immediately). Starting graceful server shutdown immediately
            // would lead to all this traffic to get dropped.
            //
            // If the load balancer *is* aware of the API server termination, we'll wait for `SHUTDOWN_INTERVAL_WITHOUT_REQUESTS`,
            // which is fairly short.
            let wait_result = tokio::time::timeout(
                NO_REQUESTS_WAIT_TIMEOUT,
                traffic_tracker.wait_for_no_requests(SHUTDOWN_INTERVAL_WITHOUT_REQUESTS),
            )
            .await;

            if wait_result.is_err() {
                tracing::warn!(
                    "Timed out waiting {NO_REQUESTS_WAIT_TIMEOUT:?} for traffic to be stopped by load balancer"
                );
            }
            tracing::info!("Stopping serving new {transport_str} traffic");
            if let Some(closing_vm_barrier) = closing_vm_barrier {
                closing_vm_barrier.close();
            }
            close_handle.stop().ok();
        });

        server_handle.stopped().await;
        drop(health_updater);
        tracing::info!("{transport_str} JSON-RPC server stopped");
        if let Some(vm_barrier) = vm_barrier {
            Self::wait_for_vm(vm_barrier, transport_str).await;
        }
        Ok(())
    }
}
