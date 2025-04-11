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
use zksync_config::configs::api::{MaxResponseSize, MaxResponseSizeOverrides};
use zksync_dal::{helpers::wait_for_l1_batch, ConnectionPool, Core};
use zksync_health_check::{HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_metadata_calculator::api_server::TreeApiClient;
use zksync_node_sync::SyncState;
use zksync_types::L2BlockNumber;
use zksync_web3_decl::{
    client::{DynClient, L2},
    jsonrpsee::{
        server::{
            middleware::rpc::either::Either, BatchRequestConfig, RpcServiceBuilder, ServerBuilder,
        },
        MethodCallback, Methods, RpcModule,
    },
    namespaces::{
        DebugNamespaceServer, EnNamespaceServer, EthNamespaceServer, EthPubSubServer,
        NetNamespaceServer, SnapshotsNamespaceServer, UnstableNamespaceServer, Web3NamespaceServer,
        ZksNamespaceServer,
    },
    types::Filter,
};

use self::{
    backend_jsonrpsee::{
        CorrelationMiddleware, LimitMiddleware, MetadataLayer, MethodTracer, ShutdownMiddleware,
        TrafficTracker,
    },
    mempool_cache::MempoolCache,
    metrics::API_METRICS,
    namespaces::{
        DebugNamespace, EnNamespace, EthNamespace, NetNamespace, SnapshotsNamespace,
        UnstableNamespace, Web3Namespace, ZksNamespace,
    },
    pubsub::{EthSubscribe, EthSubscriptionIdProvider, PubSubEvent},
    state::{Filters, InternalApiConfig, RpcState, SealedL2BlockNumber},
};
use crate::{
    execution_sandbox::{BlockStartInfo, VmConcurrencyBarrier},
    tx_sender::TxSender,
    utils::AccountTypesCache,
    web3::state::BridgeAddressesHandle,
};

pub mod backend_jsonrpsee;
pub mod mempool_cache;
pub(super) mod metrics;
pub mod namespaces;
mod pubsub;
pub mod state;
pub mod testonly;
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
    Events(Filter, L2BlockNumber),
    // Blocks from some block
    Blocks(L2BlockNumber),
    // Pending transactions from some timestamp
    PendingTransactions(NaiveDateTime),
}

#[derive(Debug, Clone, Copy)]
enum ApiTransport {
    WebSocket(SocketAddr),
    Http(SocketAddr),
}

#[derive(Debug, Deserialize, Clone, PartialEq, strum::EnumString)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum Namespace {
    Eth,
    Net,
    Web3,
    Debug,
    Zks,
    En,
    Pubsub,
    Snapshots,
    Unstable,
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
    response_body_size_limit: Option<MaxResponseSize>,
    websocket_requests_per_minute_limit: Option<NonZeroU32>,
    tree_api: Option<Arc<dyn TreeApiClient>>,
    mempool_cache: Option<MempoolCache>,
    extended_tracing: bool,
    pub_sub_events_sender: Option<mpsc::UnboundedSender<PubSubEvent>>,
    l2_l1_log_proof_handler: Option<Box<DynClient<L2>>>,
}

/// Structure capable of spawning a configured Web3 API server along with all the required
/// maintenance tasks.
#[derive(Debug)]
pub struct ApiServer {
    pool: ConnectionPool<Core>,
    health_updater: Arc<HealthUpdater>,
    config: InternalApiConfig,
    transport: ApiTransport,
    tx_sender: TxSender,
    polling_interval: Duration,
    pruning_info_refresh_interval: Duration,
    namespaces: Vec<Namespace>,
    method_tracer: Arc<MethodTracer>,
    optional: OptionalApiParams,
    bridge_addresses_handle: BridgeAddressesHandle,
    sealed_l2_block_handle: SealedL2BlockNumber,
}

#[derive(Debug)]
pub struct ApiBuilder {
    pool: ConnectionPool<Core>,
    config: InternalApiConfig,
    polling_interval: Duration,
    pruning_info_refresh_interval: Duration,
    // Mandatory params that must be set using builder methods.
    transport: Option<ApiTransport>,
    tx_sender: Option<TxSender>,
    bridge_addresses_handle: Option<BridgeAddressesHandle>,
    sealed_l2_block_handle: Option<SealedL2BlockNumber>,
    // Optional params that may or may not be set using builder methods. We treat `namespaces`
    // specially because we want to output a warning if they are not set.
    namespaces: Option<Vec<Namespace>>,
    method_tracer: Arc<MethodTracer>,
    optional: OptionalApiParams,
}

impl ApiBuilder {
    const DEFAULT_POLLING_INTERVAL: Duration = Duration::from_millis(200);
    const DEFAULT_PRUNING_INFO_REFRESH_INTERVAL: Duration = Duration::from_secs(10);

    pub fn jsonrpsee_backend(config: InternalApiConfig, pool: ConnectionPool<Core>) -> Self {
        Self {
            pool,
            config,
            polling_interval: Self::DEFAULT_POLLING_INTERVAL,
            pruning_info_refresh_interval: Self::DEFAULT_PRUNING_INFO_REFRESH_INTERVAL,
            transport: None,
            tx_sender: None,
            bridge_addresses_handle: None,
            sealed_l2_block_handle: None,
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

    pub fn with_response_body_size_limit(mut self, max_response_size: MaxResponseSize) -> Self {
        self.optional.response_body_size_limit = Some(max_response_size);
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

    pub fn with_pruning_info_refresh_interval(mut self, interval: Duration) -> Self {
        self.pruning_info_refresh_interval = interval;
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

    pub fn with_mempool_cache(mut self, cache: MempoolCache) -> Self {
        self.optional.mempool_cache = Some(cache);
        self
    }

    pub fn with_extended_tracing(mut self, extended_tracing: bool) -> Self {
        self.optional.extended_tracing = extended_tracing;
        self
    }

    pub fn with_sealed_l2_block_handle(
        mut self,
        sealed_l2_block_handle: SealedL2BlockNumber,
    ) -> Self {
        self.sealed_l2_block_handle = Some(sealed_l2_block_handle);
        self
    }

    pub fn with_bridge_addresses_handle(
        mut self,
        bridge_addresses_handle: BridgeAddressesHandle,
    ) -> Self {
        self.bridge_addresses_handle = Some(bridge_addresses_handle);
        self
    }

    pub fn with_l2_l1_log_proof_handler(
        mut self,
        l2_l1_log_proof_handler: Box<DynClient<L2>>,
    ) -> Self {
        self.optional.l2_l1_log_proof_handler = Some(l2_l1_log_proof_handler);
        self
    }

    // Intended for tests only.
    #[doc(hidden)]
    fn with_pub_sub_events(mut self, sender: mpsc::UnboundedSender<PubSubEvent>) -> Self {
        self.optional.pub_sub_events_sender = Some(sender);
        self
    }

    // Intended for tests only.
    #[doc(hidden)]
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
            config: self.config,
            transport,
            tx_sender: self.tx_sender.context("Transaction sender not set")?,
            polling_interval: self.polling_interval,
            pruning_info_refresh_interval: self.pruning_info_refresh_interval,
            namespaces: self.namespaces.unwrap_or_else(|| {
                tracing::warn!(
                    "debug_ and snapshots_ API namespace will be disabled by default in ApiBuilder"
                );
                Namespace::DEFAULT.to_vec()
            }),
            method_tracer: self.method_tracer,
            optional: self.optional,
            sealed_l2_block_handle: self
                .sealed_l2_block_handle
                .context("Sealed l2 block handle not set")?,
            bridge_addresses_handle: self
                .bridge_addresses_handle
                .context("Bridge addresses handle not set")?,
        })
    }
}

impl ApiServer {
    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.health_updater.subscribe()
    }

    async fn build_rpc_state(self) -> anyhow::Result<RpcState> {
        let mut storage = self.pool.connection_tagged("api").await?;
        let start_info =
            BlockStartInfo::new(&mut storage, self.pruning_info_refresh_interval).await?;
        drop(storage);

        // Disable filter API for HTTP endpoints, WS endpoints are unaffected by the `filters_disabled` flag
        let installed_filters =
            if matches!(self.transport, ApiTransport::Http(_)) && self.config.filters_disabled {
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
            mempool_cache: self.optional.mempool_cache,
            account_types_cache: AccountTypesCache::default(),
            last_sealed_l2_block: self.sealed_l2_block_handle,
            bridge_addresses_handle: self.bridge_addresses_handle,
            tree_api: self.optional.tree_api,
            l2_l1_log_proof_handler: self.optional.l2_l1_log_proof_handler,
        })
    }

    async fn build_rpc_module(
        self,
        pub_sub: Option<EthSubscribe>,
    ) -> anyhow::Result<RpcModule<()>> {
        let namespaces = self.namespaces.clone();
        let zksync_network_id = self.config.l2_chain_id;
        let rpc_state = self.build_rpc_state().await?;

        // Collect all the methods into a single RPC module.
        let mut rpc = RpcModule::new(());
        if let Some(pub_sub) = pub_sub {
            rpc.merge(pub_sub.into_rpc())
                .context("cannot merge eth pubsub namespace")?;
        }

        if namespaces.contains(&Namespace::Debug) {
            rpc.merge(DebugNamespace::new(rpc_state.clone()).await?.into_rpc())
                .context("cannot merge debug namespace")?;
        }
        if namespaces.contains(&Namespace::Eth) {
            rpc.merge(EthNamespace::new(rpc_state.clone()).into_rpc())
                .context("cannot merge eth namespace")?;
        }
        if namespaces.contains(&Namespace::Net) {
            rpc.merge(NetNamespace::new(zksync_network_id).into_rpc())
                .context("cannot merge net namespace")?;
        }
        if namespaces.contains(&Namespace::Web3) {
            rpc.merge(Web3Namespace.into_rpc())
                .context("cannot merge web3 namespace")?;
        }
        if namespaces.contains(&Namespace::Zks) {
            rpc.merge(ZksNamespace::new(rpc_state.clone()).into_rpc())
                .context("cannot merge zks namespace")?;
        }
        if namespaces.contains(&Namespace::En) {
            rpc.merge(EnNamespace::new(rpc_state.clone()).into_rpc())
                .context("cannot merge en namespace")?;
        }
        if namespaces.contains(&Namespace::Snapshots) {
            rpc.merge(SnapshotsNamespace::new(rpc_state.clone()).into_rpc())
                .context("cannot merge snapshots namespace")?;
        }
        if namespaces.contains(&Namespace::Unstable) {
            rpc.merge(UnstableNamespace::new(rpc_state).into_rpc())
                .context("cannot merge unstable namespace")?;
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
        let transport = self.transport;
        let mut tasks = vec![];

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
        let server_task =
            tokio::spawn(self.run_jsonrpsee_server(stop_receiver, pub_sub, local_addr_sender));

        tasks.push(server_task);
        Ok(ApiServerHandles {
            health_check,
            tasks,
            local_addr: future::try_maybe_done(local_addr),
        })
    }

    /// Overrides max response sizes for specific RPC methods by additionally wrapping their callbacks
    /// to which the max response size is passed as a param.
    fn override_method_response_sizes(
        rpc: RpcModule<()>,
        response_size_overrides: &MaxResponseSizeOverrides,
    ) -> anyhow::Result<Methods> {
        let rpc = Methods::from(rpc);
        let mut output_rpc = Methods::new();

        for method_name in rpc.method_names() {
            let method = rpc
                .method(method_name)
                .with_context(|| format!("method `{method_name}` disappeared from RPC module"))?;
            let response_size_limit = response_size_overrides.get(method_name);

            let method = match (method, response_size_limit) {
                (MethodCallback::Sync(sync_method), Some(limit)) => {
                    tracing::info!(
                        "Overriding max response size to {limit}B for sync method `{method_name}`"
                    );
                    let sync_method = sync_method.clone();
                    MethodCallback::Sync(Arc::new(move |id, params, _max_response_size, ext| {
                        sync_method(id, params, limit, ext)
                    }))
                }
                (MethodCallback::Async(async_method), Some(limit)) => {
                    tracing::info!(
                        "Overriding max response size to {limit}B for async method `{method_name}`"
                    );
                    let async_method = async_method.clone();
                    MethodCallback::Async(Arc::new(
                        move |id, params, connection_id, _max_response_size, ext| {
                            async_method(id, params, connection_id, limit, ext)
                        },
                    ))
                }
                (MethodCallback::Unsubscription(unsub_method), Some(limit)) => {
                    tracing::info!(
                        "Overriding max response size to {limit}B for unsub method `{method_name}`"
                    );
                    let unsub_method = unsub_method.clone();
                    MethodCallback::Unsubscription(Arc::new(
                        move |id, params, connection_id, _max_response_size, ext| {
                            unsub_method(id, params, connection_id, limit, ext)
                        },
                    ))
                }
                _ => method.clone(),
            };
            output_rpc.verify_and_insert(method_name, method)?;
        }

        Ok(output_rpc)
    }

    async fn run_jsonrpsee_server(
        self,
        mut stop_receiver: watch::Receiver<bool>,
        pub_sub: Option<EthSubscribe>,
        local_addr_sender: oneshot::Sender<SocketAddr>,
    ) -> anyhow::Result<()> {
        let transport = self.transport;
        let (transport_str, is_http, addr) = match transport {
            ApiTransport::Http(addr) => ("HTTP", true, addr),
            ApiTransport::WebSocket(addr) => ("WS", false, addr),
        };
        let transport_label = (&transport).into();
        API_METRICS.observe_config(
            transport_label,
            self.polling_interval,
            &self.config,
            &self.optional,
        );

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
        let (response_body_size_limit, max_response_size_overrides) =
            if let Some(limit) = &self.optional.response_body_size_limit {
                (limit.global as u32, limit.overrides.clone())
            } else {
                (u32::MAX, MaxResponseSizeOverrides::empty())
            };
        let websocket_requests_per_minute_limit = self.optional.websocket_requests_per_minute_limit;
        let subscriptions_limit = self.optional.subscriptions_limit;
        let vm_barrier = self.optional.vm_barrier.clone();
        let health_updater = self.health_updater.clone();
        let method_tracer = self.method_tracer.clone();

        let extended_tracing = self.optional.extended_tracing;
        if extended_tracing {
            tracing::info!("Enabled extended call tracing for {transport_str} API server; this might negatively affect performance");
        }

        let rpc = self.build_rpc_module(pub_sub).await?;
        let registered_method_names = Arc::new(rpc.method_names().collect::<HashSet<_>>());
        tracing::debug!(
            "Built RPC module for {transport_str} server with {} methods: {registered_method_names:?}",
            registered_method_names.len()
        );
        let rpc = Self::override_method_response_sizes(rpc, &max_response_size_overrides)?;

        // Setup CORS.
        let cors = is_http.then(|| {
            CorsLayer::new()
                // Allow `POST` when accessing the resource
                .allow_methods([http::Method::POST])
                // Allow requests from any origin
                .allow_origin(tower_http::cors::Any)
                .allow_headers([http::header::CONTENT_TYPE])
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

        let metadata_layer = MetadataLayer::new(registered_method_names, method_tracer);
        let metadata_layer = if extended_tracing {
            Either::Left(metadata_layer.with_param_tracing())
        } else {
            Either::Right(metadata_layer)
        };
        let traffic_tracker = TrafficTracker::default();
        let traffic_tracker_for_middleware = traffic_tracker.clone();

        // **Important.** The ordering of layers matters! Layers added first will receive the request earlier
        // (i.e., are outermost in the call chain).
        let rpc_middleware = RpcServiceBuilder::new()
            .layer_fn(move |svc| {
                ShutdownMiddleware::new(svc, traffic_tracker_for_middleware.clone())
            })
            // We want to output method logs with a correlation ID; hence, `CorrelationMiddleware` must precede `metadata_layer`.
            .option_layer(
                extended_tracing.then(|| tower::layer::layer_fn(CorrelationMiddleware::new)),
            )
            .layer(metadata_layer)
            // We want to capture limit middleware errors with `metadata_layer`; hence, `LimitMiddleware` is placed after it.
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
