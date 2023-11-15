use anyhow::Context as _;
use futures::future;
use jsonrpc_core::MetaIoHandler;
use jsonrpc_http_server::hyper;
use jsonrpc_pubsub::PubSubHandler;
use serde::Deserialize;
use tokio::sync::{oneshot, watch, RwLock};
use tower_http::{cors::CorsLayer, metrics::InFlightRequestsLayer};

use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::task::JoinHandle;

use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_health_check::{HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_types::{api, MiniblockNumber};
use zksync_web3_decl::{
    error::Web3Error,
    jsonrpsee::{
        server::{BatchRequestConfig, ServerBuilder},
        RpcModule,
    },
    namespaces::{
        DebugNamespaceServer, EnNamespaceServer, EthNamespaceServer, NetNamespaceServer,
        Web3NamespaceServer, ZksNamespaceServer,
    },
};

use crate::{
    api_server::{
        execution_sandbox::VmConcurrencyBarrier, tree::TreeApiHttpClient, tx_sender::TxSender,
        web3::backend_jsonrpc::batch_limiter_middleware::RateLimitMetadata,
    },
    l1_gas_price::L1GasPriceProvider,
    sync_layer::SyncState,
};

pub mod backend_jsonrpc;
pub mod backend_jsonrpsee;
mod metrics;
pub mod namespaces;
mod pubsub_notifier;
pub mod state;
#[cfg(test)]
pub(crate) mod tests;

use self::backend_jsonrpc::{
    batch_limiter_middleware::{LimitMiddleware, Transport},
    error::internal_error,
    namespaces::{
        debug::DebugNamespaceT, en::EnNamespaceT, eth::EthNamespaceT, net::NetNamespaceT,
        web3::Web3NamespaceT, zks::ZksNamespaceT,
    },
    pub_sub::Web3PubSub,
};
use self::metrics::API_METRICS;
use self::namespaces::{
    DebugNamespace, EnNamespace, EthNamespace, EthSubscribe, NetNamespace, Web3Namespace,
    ZksNamespace,
};
use self::pubsub_notifier::{notify_blocks, notify_logs, notify_txs};
use self::state::{Filters, InternalApiConfig, RpcState, SealedMiniblockNumber};

/// Timeout for graceful shutdown logic within API servers.
const GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy)]
enum ApiBackend {
    Jsonrpsee,
    Jsonrpc,
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
}

impl Namespace {
    pub const ALL: &'static [Namespace] = &[
        Namespace::Eth,
        Namespace::Net,
        Namespace::Web3,
        Namespace::Debug,
        Namespace::Zks,
        Namespace::En,
        Namespace::Pubsub,
    ];

    pub const NON_DEBUG: &'static [Namespace] = &[
        Namespace::Eth,
        Namespace::Net,
        Namespace::Web3,
        Namespace::Zks,
        Namespace::En,
        Namespace::Pubsub,
    ];
}

/// Handles to the initialized API server.
#[derive(Debug)]
pub struct ApiServerHandles {
    pub local_addr: SocketAddr,
    pub tasks: Vec<JoinHandle<anyhow::Result<()>>>,
    pub health_check: ReactiveHealthCheck,
}

#[derive(Debug)]
pub struct ApiBuilder<G> {
    backend: ApiBackend,
    pool: ConnectionPool,
    last_miniblock_pool: ConnectionPool,
    config: InternalApiConfig,
    transport: Option<ApiTransport>,
    tx_sender: Option<TxSender<G>>,
    vm_barrier: Option<VmConcurrencyBarrier>,
    filters_limit: Option<usize>,
    subscriptions_limit: Option<usize>,
    batch_request_size_limit: Option<usize>,
    response_body_size_limit: Option<usize>,
    websocket_requests_per_minute_limit: Option<u32>,
    sync_state: Option<SyncState>,
    threads: Option<usize>,
    vm_concurrency_limit: Option<usize>,
    polling_interval: Option<Duration>,
    namespaces: Option<Vec<Namespace>>,
    logs_translator_enabled: bool,
    tree_api_url: Option<String>,
}

impl<G> ApiBuilder<G> {
    pub fn jsonrpsee_backend(config: InternalApiConfig, pool: ConnectionPool) -> Self {
        Self {
            backend: ApiBackend::Jsonrpsee,
            transport: None,
            last_miniblock_pool: pool.clone(),
            pool,
            sync_state: None,
            tx_sender: None,
            vm_barrier: None,
            filters_limit: None,
            subscriptions_limit: None,
            batch_request_size_limit: None,
            response_body_size_limit: None,
            websocket_requests_per_minute_limit: None,
            threads: None,
            vm_concurrency_limit: None,
            polling_interval: None,
            namespaces: None,
            config,
            logs_translator_enabled: false,
            tree_api_url: None,
        }
    }

    pub fn jsonrpc_backend(config: InternalApiConfig, pool: ConnectionPool) -> Self {
        Self {
            backend: ApiBackend::Jsonrpc,
            ..Self::jsonrpsee_backend(config, pool)
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

    /// Configures a dedicated DB pool to be used for updating the latest miniblock information
    /// in a background task. If not called, the main pool will be used. If the API server is under high load,
    /// it may make sense to supply a single-connection pool to reduce pool contention with the API methods.
    pub fn with_last_miniblock_pool(mut self, pool: ConnectionPool) -> Self {
        self.last_miniblock_pool = pool;
        self
    }

    pub fn with_tx_sender(
        mut self,
        tx_sender: TxSender<G>,
        vm_barrier: VmConcurrencyBarrier,
    ) -> Self {
        self.tx_sender = Some(tx_sender);
        self.vm_barrier = Some(vm_barrier);
        self
    }

    pub fn with_filter_limit(mut self, filters_limit: usize) -> Self {
        self.filters_limit = Some(filters_limit);
        self
    }

    pub fn with_subscriptions_limit(mut self, subscriptions_limit: usize) -> Self {
        self.subscriptions_limit = Some(subscriptions_limit);
        self
    }

    pub fn with_batch_request_size_limit(mut self, batch_request_size_limit: usize) -> Self {
        self.batch_request_size_limit = Some(batch_request_size_limit);
        self
    }

    pub fn with_response_body_size_limit(mut self, response_body_size_limit: usize) -> Self {
        self.response_body_size_limit = Some(response_body_size_limit);
        self
    }

    pub fn with_websocket_requests_per_minute_limit(
        mut self,
        websocket_requests_per_minute_limit: u32,
    ) -> Self {
        self.websocket_requests_per_minute_limit = Some(websocket_requests_per_minute_limit);
        self
    }

    pub fn with_sync_state(mut self, sync_state: SyncState) -> Self {
        self.sync_state = Some(sync_state);
        self
    }

    pub fn with_threads(mut self, threads: usize) -> Self {
        self.threads = Some(threads);
        self
    }

    pub fn with_polling_interval(mut self, polling_interval: Duration) -> Self {
        self.polling_interval = Some(polling_interval);
        self
    }

    pub fn with_vm_concurrency_limit(mut self, vm_concurrency_limit: usize) -> Self {
        self.vm_concurrency_limit = Some(vm_concurrency_limit);
        self
    }

    pub fn enable_api_namespaces(mut self, namespaces: Vec<Namespace>) -> Self {
        self.namespaces = Some(namespaces);
        self
    }

    pub fn enable_request_translator(mut self) -> Self {
        tracing::info!("Logs request translator enabled");
        self.logs_translator_enabled = true;
        self
    }

    pub fn with_tree_api(mut self, tree_api_url: Option<String>) -> Self {
        self.tree_api_url = tree_api_url;
        self
    }
}

impl<G: 'static + Send + Sync + L1GasPriceProvider> ApiBuilder<G> {
    fn build_rpc_state(self) -> RpcState<G> {
        // Chosen to be significantly smaller than the interval between miniblocks, but larger than
        // the latency of getting the latest sealed miniblock number from Postgres. If the API server
        // processes enough requests, information about the latest sealed miniblock will be updated
        // by reporting block difference metrics, so the actual update lag would be much smaller than this value.
        const SEALED_MINIBLOCK_UPDATE_INTERVAL: Duration = Duration::from_millis(25);

        let (last_sealed_miniblock, update_task) =
            SealedMiniblockNumber::new(self.last_miniblock_pool, SEALED_MINIBLOCK_UPDATE_INTERVAL);
        // The update tasks takes care of its termination, so we don't need to retain its handle.
        tokio::spawn(update_task);

        RpcState {
            installed_filters: Arc::new(RwLock::new(Filters::new(
                self.filters_limit.unwrap_or(usize::MAX),
            ))),
            connection_pool: self.pool,
            tx_sender: self.tx_sender.expect("TxSender is not provided"),
            sync_state: self.sync_state,
            api_config: self.config,
            last_sealed_miniblock,
            logs_translator_enabled: self.logs_translator_enabled,
            tree_api: self
                .tree_api_url
                .map(|url| TreeApiHttpClient::new(url.as_str())),
        }
    }

    async fn build_rpc_module(mut self) -> RpcModule<()> {
        let namespaces = self.namespaces.take().unwrap();
        let zksync_network_id = self.config.l2_chain_id;
        let rpc_state = self.build_rpc_state();

        // Collect all the methods into a single RPC module.
        let mut rpc = RpcModule::new(());
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
            rpc.merge(DebugNamespace::new(rpc_state).await.into_rpc())
                .expect("Can't merge debug namespace");
        }
        rpc
    }

    pub async fn build(
        mut self,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<ApiServerHandles> {
        if self.filters_limit.is_none() {
            tracing::warn!("Filters limit is not set - unlimited filters are allowed");
        }

        if self.namespaces.is_none() {
            tracing::warn!("debug_ API namespace will be disabled by default in ApiBuilder");
            self.namespaces = Some(Namespace::NON_DEBUG.to_vec());
        }

        if self
            .namespaces
            .as_ref()
            .unwrap()
            .contains(&Namespace::Pubsub)
            && matches!(&self.transport, Some(ApiTransport::Http(_)))
        {
            tracing::debug!("pubsub API is not supported for HTTP transport, ignoring");
        }

        match (&self.transport, self.subscriptions_limit) {
            (Some(ApiTransport::WebSocket(_)), None) => {
                tracing::warn!(
                    "`subscriptions_limit` is not set - unlimited subscriptions are allowed"
                );
            }
            (Some(ApiTransport::Http(_)), Some(_)) => {
                tracing::warn!(
                    "`subscriptions_limit` is ignored for HTTP transport, use WebSocket instead"
                );
            }
            _ => {}
        }

        match (self.backend, self.transport.take()) {
            (ApiBackend::Jsonrpc, Some(ApiTransport::Http(addr))) => {
                self.build_jsonrpc_http(addr, stop_receiver).await
            }
            (ApiBackend::Jsonrpc, Some(ApiTransport::WebSocket(addr))) => {
                self.build_jsonrpc_ws(addr, stop_receiver).await
            }
            (ApiBackend::Jsonrpsee, Some(transport)) => {
                self.build_jsonrpsee(transport, stop_receiver).await
            }
            (_, None) => anyhow::bail!("ApiTransport is not specified"),
        }
    }

    async fn build_jsonrpc_http(
        mut self,
        addr: SocketAddr,
        mut stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<ApiServerHandles> {
        if self.batch_request_size_limit.is_some() {
            tracing::info!("`batch_request_size_limit` is not supported for HTTP `jsonrpc` backend, this value is ignored");
        }
        if self.response_body_size_limit.is_some() {
            tracing::info!("`response_body_size_limit` is not supported for `jsonrpc` backend, this value is ignored");
        }

        let (health_check, health_updater) = ReactiveHealthCheck::new("http_api");
        let vm_barrier = self.vm_barrier.take().unwrap();
        // ^ `unwrap()` is safe by construction

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("jsonrpc-http-worker")
            .worker_threads(self.threads.unwrap())
            .build()
            .context("Failed creating Tokio runtime for `jsonrpc` API backend")?;
        let mut io_handler: MetaIoHandler<()> = MetaIoHandler::default();
        self.extend_jsonrpc_methods(&mut io_handler).await;

        let (local_addr_sender, local_addr) = oneshot::channel();
        let server_task = tokio::task::spawn_blocking(move || {
            let server = jsonrpc_http_server::ServerBuilder::new(io_handler)
                .threads(1)
                .event_loop_executor(runtime.handle().clone())
                .start_http(&addr)
                .context("jsonrpc_http::Server::start_http")?;
            local_addr_sender.send(*server.address()).ok();

            let close_handle = server.close_handle();
            let closing_vm_barrier = vm_barrier.clone();
            runtime.handle().spawn(async move {
                if stop_receiver.changed().await.is_err() {
                    tracing::warn!(
                        "Stop signal sender for HTTP JSON-RPC server was dropped without sending a signal"
                    );
                }
                tracing::info!("Stop signal received, HTTP JSON-RPC server is shutting down");
                closing_vm_barrier.close();
                close_handle.close();
            });

            health_updater.update(HealthStatus::Ready.into());
            server.wait();
            drop(health_updater);
            tracing::info!("HTTP JSON-RPC server stopped");
            runtime.block_on(Self::wait_for_vm(vm_barrier, "HTTP"));
            runtime.shutdown_timeout(GRACEFUL_SHUTDOWN_TIMEOUT);
            Ok(())
        });

        let local_addr = match local_addr.await {
            Ok(addr) => addr,
            Err(_) => {
                // If the local address was not transmitted, `server_task` must have failed.
                let err = server_task
                    .await
                    .context("HTTP JSON-RPC server panicked")?
                    .unwrap_err();
                return Err(err);
            }
        };
        Ok(ApiServerHandles {
            local_addr,
            health_check,
            tasks: vec![server_task],
        })
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

    async fn extend_jsonrpc_methods<T, S>(mut self, io: &mut MetaIoHandler<T, S>)
    where
        T: jsonrpc_core::Metadata,
        S: jsonrpc_core::Middleware<T>,
    {
        let zksync_network_id = self.config.l2_chain_id;
        let namespaces = self.namespaces.take().unwrap();
        let rpc_state = self.build_rpc_state();
        if namespaces.contains(&Namespace::Eth) {
            io.extend_with(EthNamespace::new(rpc_state.clone()).to_delegate());
        }
        if namespaces.contains(&Namespace::Zks) {
            io.extend_with(ZksNamespace::new(rpc_state.clone()).to_delegate());
        }
        if namespaces.contains(&Namespace::En) {
            io.extend_with(EnNamespace::new(rpc_state.clone()).to_delegate());
        }
        if namespaces.contains(&Namespace::Web3) {
            io.extend_with(Web3Namespace.to_delegate());
        }
        if namespaces.contains(&Namespace::Net) {
            io.extend_with(NetNamespace::new(zksync_network_id).to_delegate());
        }
        if namespaces.contains(&Namespace::Debug) {
            let debug_ns = DebugNamespace::new(rpc_state).await;
            io.extend_with(debug_ns.to_delegate());
        }
    }

    async fn build_jsonrpc_ws(
        mut self,
        addr: SocketAddr,
        mut stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<ApiServerHandles> {
        if self.response_body_size_limit.is_some() {
            tracing::info!("`response_body_size_limit` is not supported for `jsonrpc` backend, this value is ignored");
        }

        let (health_check, health_updater) = ReactiveHealthCheck::new("ws_api");
        let websocket_requests_per_second_limit = self.websocket_requests_per_minute_limit;
        let batch_limiter_middleware =
            LimitMiddleware::new(Transport::Ws, self.batch_request_size_limit);

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("jsonrpc-ws-worker")
            .worker_threads(self.threads.unwrap())
            .build()
            .context("Failed creating Tokio runtime for `jsonrpc-ws` API backend")?;
        let max_connections = self.subscriptions_limit.unwrap_or(usize::MAX);
        let vm_barrier = self.vm_barrier.take().unwrap();

        let io_handler: MetaIoHandler<RateLimitMetadata<Arc<jsonrpc_pubsub::Session>>, _> =
            MetaIoHandler::with_middleware(batch_limiter_middleware);
        let mut io_handler = PubSubHandler::new(io_handler);
        let mut tasks = Vec::new();

        if self
            .namespaces
            .as_ref()
            .unwrap()
            .contains(&Namespace::Pubsub)
        {
            let pub_sub = EthSubscribe::new(runtime.handle().clone());
            let polling_interval = self
                .polling_interval
                .context("Polling interval is not set")?;
            tasks.extend([
                tokio::spawn(notify_blocks(
                    pub_sub.active_block_subs.clone(),
                    self.pool.clone(),
                    polling_interval,
                    stop_receiver.clone(),
                )),
                tokio::spawn(notify_txs(
                    pub_sub.active_tx_subs.clone(),
                    self.pool.clone(),
                    polling_interval,
                    stop_receiver.clone(),
                )),
                tokio::spawn(notify_logs(
                    pub_sub.active_log_subs.clone(),
                    self.pool.clone(),
                    polling_interval,
                    stop_receiver.clone(),
                )),
            ]);
            io_handler.extend_with(pub_sub.to_delegate());
        }
        self.extend_jsonrpc_methods(&mut io_handler).await;

        let (local_addr_sender, local_addr) = oneshot::channel();
        let server_task = tokio::task::spawn_blocking(move || {
            let server = jsonrpc_ws_server::ServerBuilder::with_meta_extractor(
                io_handler,
                move |context: &jsonrpc_ws_server::RequestContext| {
                    let session = Arc::new(jsonrpc_pubsub::Session::new(context.sender()));
                    RateLimitMetadata::new(websocket_requests_per_second_limit, session)
                },
            )
            .event_loop_executor(runtime.handle().clone())
            .max_connections(max_connections)
            .session_stats(TrackOpenWsConnections)
            .start(&addr)
            .context("jsonrpc_ws_server::Server::start()")?;

            local_addr_sender.send(*server.addr()).ok();

            let close_handle = server.close_handle();
            let closing_vm_barrier = vm_barrier.clone();
            runtime.handle().spawn(async move {
                if stop_receiver.changed().await.is_err() {
                    tracing::warn!(
                        "Stop signal sender for WS JSON-RPC server was dropped without sending a signal"
                    );
                }
                tracing::info!("Stop signal received, WS JSON-RPC server is shutting down");
                closing_vm_barrier.close();
                close_handle.close();
            });

            health_updater.update(HealthStatus::Ready.into());
            server
                .wait()
                .context("WS JSON-RPC server encountered fatal error")?;
            drop(health_updater);
            tracing::info!("WS JSON-RPC server stopped");
            runtime.block_on(Self::wait_for_vm(vm_barrier, "WS"));
            runtime.shutdown_timeout(GRACEFUL_SHUTDOWN_TIMEOUT);
            Ok(())
        });

        let local_addr = match local_addr.await {
            Ok(addr) => addr,
            Err(_) => {
                // If the local address was not transmitted, `server_task` must have failed.
                let err = server_task
                    .await
                    .context("WS JSON-RPC server panicked")?
                    .unwrap_err();
                return Err(err);
            }
        };
        tasks.push(server_task);

        Ok(ApiServerHandles {
            local_addr,
            tasks,
            health_check,
        })
    }

    async fn build_jsonrpsee(
        mut self,
        transport: ApiTransport,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<ApiServerHandles> {
        if matches!(transport, ApiTransport::WebSocket(_)) {
            // TODO (SMA-1588): Implement `eth_subscribe` method for `jsonrpsee`.
            tracing::warn!(
                "`eth_subscribe` is not implemented for jsonrpsee backend, use jsonrpc instead"
            );
            if self.websocket_requests_per_minute_limit.is_some() {
                tracing::info!("`websocket_requests_per_second_limit` is not supported for `jsonrpsee` backend, this value is ignored");
            }
        }

        let (runtime_thread_name, health_check_name) = match transport {
            ApiTransport::Http(_) => ("jsonrpsee-http-worker", "http_api"),
            ApiTransport::WebSocket(_) => ("jsonrpsee-ws-worker", "ws_api"),
        };
        let (health_check, health_updater) = ReactiveHealthCheck::new(health_check_name);
        let vm_barrier = self.vm_barrier.take().unwrap();
        let batch_request_config = if let Some(limit) = self.batch_request_size_limit {
            BatchRequestConfig::Limit(limit as u32)
        } else {
            BatchRequestConfig::Unlimited
        };
        let response_body_size_limit = self
            .response_body_size_limit
            .map(|limit| limit as u32)
            .unwrap_or(u32::MAX);

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name(runtime_thread_name)
            .worker_threads(self.threads.unwrap())
            .build()
            .with_context(|| {
                format!("Failed creating Tokio runtime for {health_check_name} jsonrpsee server")
            })?;
        let rpc = self.build_rpc_module().await;

        // Start the server in a separate tokio runtime from a dedicated thread.
        let (local_addr_sender, local_addr) = oneshot::channel();
        let server_task = tokio::task::spawn_blocking(move || {
            let res = runtime.block_on(Self::run_jsonrpsee_server(
                rpc,
                transport,
                stop_receiver,
                local_addr_sender,
                health_updater,
                vm_barrier,
                batch_request_config,
                response_body_size_limit,
            ));
            runtime.shutdown_timeout(GRACEFUL_SHUTDOWN_TIMEOUT);
            res
        });

        let local_addr = match local_addr.await {
            Ok(addr) => addr,
            Err(_) => {
                // If the local address was not transmitted, `server_task` must have failed.
                let err = server_task
                    .await
                    .with_context(|| format!("{health_check_name} server panicked"))?
                    .unwrap_err();
                return Err(err);
            }
        };
        Ok(ApiServerHandles {
            local_addr,
            health_check,
            tasks: vec![server_task],
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_jsonrpsee_server(
        rpc: RpcModule<()>,
        transport: ApiTransport,
        mut stop_receiver: watch::Receiver<bool>,
        local_addr_sender: oneshot::Sender<SocketAddr>,
        health_updater: HealthUpdater,
        vm_barrier: VmConcurrencyBarrier,
        batch_request_config: BatchRequestConfig,
        response_body_size_limit: u32,
    ) -> anyhow::Result<()> {
        let (transport_str, is_http, addr) = match transport {
            ApiTransport::Http(addr) => ("HTTP", true, addr),
            ApiTransport::WebSocket(addr) => ("WS", false, addr),
        };
        let transport_label = (&transport).into();

        // Setup CORS.
        let cors = is_http.then(|| {
            CorsLayer::new()
                // Allow `POST` when accessing the resource
                .allow_methods([hyper::Method::POST])
                // Allow requests from any origin
                .allow_origin(tower_http::cors::Any)
                .allow_headers([hyper::header::CONTENT_TYPE])
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

        let server_builder = if is_http {
            ServerBuilder::default().http_only().max_connections(5_000)
        } else {
            ServerBuilder::default().ws_only()
        };

        let server = server_builder
            .set_batch_request_config(batch_request_config)
            .set_middleware(middleware)
            .max_response_body_size(response_body_size_limit)
            .build(addr)
            .await
            .with_context(|| format!("Failed building {transport_str} JSON-RPC server"))?;
        let local_addr = server.local_addr().with_context(|| {
            format!("Failed getting local address for {transport_str} JSON-RPC server")
        })?;
        local_addr_sender.send(local_addr).ok();
        let server_handle = server.start(rpc);

        let close_handle = server_handle.clone();
        let closing_vm_barrier = vm_barrier.clone();
        tokio::spawn(async move {
            if stop_receiver.changed().await.is_err() {
                tracing::warn!(
                    "Stop signal sender for {transport_str} JSON-RPC server was dropped \
                     without sending a signal"
                );
            }
            tracing::info!(
                "Stop signal received, {transport_str} JSON-RPC server is shutting down"
            );
            closing_vm_barrier.close();
            close_handle.stop().ok();
        });
        health_updater.update(HealthStatus::Ready.into());

        server_handle.stopped().await;
        drop(health_updater);
        tracing::info!("{transport_str} JSON-RPC server stopped");
        Self::wait_for_vm(vm_barrier, transport_str).await;
        Ok(())
    }
}

struct TrackOpenWsConnections;

impl jsonrpc_ws_server::SessionStats for TrackOpenWsConnections {
    fn open_session(&self, _id: jsonrpc_ws_server::SessionId) {
        API_METRICS.ws_open_sessions.inc_by(1);
    }

    fn close_session(&self, _id: jsonrpc_ws_server::SessionId) {
        API_METRICS.ws_open_sessions.dec_by(1);
    }
}

async fn resolve_block(
    connection: &mut StorageProcessor<'_>,
    block: api::BlockId,
    method_name: &'static str,
) -> Result<MiniblockNumber, Web3Error> {
    let result = connection.blocks_web3_dal().resolve_block_id(block).await;
    result
        .map_err(|err| internal_error(method_name, err))?
        .ok_or(Web3Error::NoBlock)
}
