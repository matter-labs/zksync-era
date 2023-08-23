// External uses
use futures::future;
use jsonrpc_core::{IoHandler, MetaIoHandler};
use jsonrpc_http_server::hyper;
use jsonrpc_pubsub::PubSubHandler;
use serde::Deserialize;
use tokio::sync::{watch, RwLock};
use tower_http::{cors::CorsLayer, metrics::InFlightRequestsLayer};

// Built-in uses
use std::{net::SocketAddr, sync::Arc, time::Duration};

// Workspace uses
use zksync_contracts::BaseSystemContractsHashes;
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

// Local uses
use crate::{
    api_server::{execution_sandbox::VmConcurrencyBarrier, tx_sender::TxSender},
    l1_gas_price::L1GasPriceProvider,
    sync_layer::SyncState,
};

pub mod backend_jsonrpc;
pub mod backend_jsonrpsee;
pub mod namespaces;
mod pubsub_notifier;
pub mod state;

// Uses from submodules.
use self::backend_jsonrpc::{
    error::internal_error,
    namespaces::{
        debug::DebugNamespaceT, en::EnNamespaceT, eth::EthNamespaceT, net::NetNamespaceT,
        web3::Web3NamespaceT, zks::ZksNamespaceT,
    },
    pub_sub::Web3PubSub,
};
use self::namespaces::{
    DebugNamespace, EnNamespace, EthNamespace, EthSubscribe, NetNamespace, Web3Namespace,
    ZksNamespace,
};
use self::pubsub_notifier::{notify_blocks, notify_logs, notify_txs};
use self::state::{Filters, InternalApiConfig, RpcState};

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

#[derive(Debug)]
pub struct ApiBuilder<G> {
    backend: ApiBackend,
    pool: ConnectionPool,
    config: InternalApiConfig,
    transport: Option<ApiTransport>,
    tx_sender: Option<TxSender<G>>,
    vm_barrier: Option<VmConcurrencyBarrier>,
    filters_limit: Option<usize>,
    subscriptions_limit: Option<usize>,
    batch_request_size_limit: Option<usize>,
    response_body_size_limit: Option<usize>,
    sync_state: Option<SyncState>,
    threads: Option<usize>,
    vm_concurrency_limit: Option<usize>,
    polling_interval: Option<Duration>,
    namespaces: Option<Vec<Namespace>>,
}

impl<G> ApiBuilder<G> {
    pub fn jsonrpsee_backend(config: InternalApiConfig, pool: ConnectionPool) -> Self {
        Self {
            backend: ApiBackend::Jsonrpsee,
            transport: None,
            pool,
            sync_state: None,
            tx_sender: None,
            vm_barrier: None,
            filters_limit: None,
            subscriptions_limit: None,
            batch_request_size_limit: None,
            response_body_size_limit: None,
            threads: None,
            vm_concurrency_limit: None,
            polling_interval: None,
            namespaces: None,
            config,
        }
    }

    pub fn jsonrpc_backend(config: InternalApiConfig, pool: ConnectionPool) -> Self {
        Self {
            backend: ApiBackend::Jsonrpc,
            transport: None,
            pool,
            sync_state: None,
            tx_sender: None,
            vm_barrier: None,
            filters_limit: None,
            subscriptions_limit: None,
            batch_request_size_limit: None,
            response_body_size_limit: None,
            threads: None,
            vm_concurrency_limit: None,
            polling_interval: None,
            namespaces: None,
            config,
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
}

impl<G: 'static + Send + Sync + L1GasPriceProvider> ApiBuilder<G> {
    fn build_rpc_state(&self) -> RpcState<G> {
        RpcState {
            installed_filters: Arc::new(RwLock::new(Filters::new(
                self.filters_limit.unwrap_or(usize::MAX),
            ))),
            connection_pool: self.pool.clone(),
            tx_sender: self.tx_sender.clone().expect("TxSender is not provided"),
            sync_state: self.sync_state.clone(),
            api_config: self.config.clone(),
        }
    }

    async fn build_rpc_module(&self) -> RpcModule<()> {
        let zksync_network_id = self.config.l2_chain_id;
        let rpc_app = self.build_rpc_state();

        // Collect all the methods into a single RPC module.
        let namespaces = self.namespaces.as_ref().unwrap();
        let mut rpc = RpcModule::new(());
        if namespaces.contains(&Namespace::Eth) {
            rpc.merge(EthNamespace::new(rpc_app.clone()).into_rpc())
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
            rpc.merge(ZksNamespace::new(rpc_app.clone()).into_rpc())
                .expect("Can't merge zks namespace");
        }
        if namespaces.contains(&Namespace::En) {
            rpc.merge(EnNamespace::new(rpc_app.clone()).into_rpc())
                .expect("Can't merge en namespace");
        }
        if namespaces.contains(&Namespace::Debug) {
            let hashes = BaseSystemContractsHashes {
                default_aa: rpc_app.tx_sender.0.sender_config.default_aa,
                bootloader: rpc_app.tx_sender.0.sender_config.bootloader,
            };
            rpc.merge(
                DebugNamespace::new(
                    rpc_app.connection_pool,
                    hashes,
                    rpc_app.tx_sender.0.sender_config.fair_l2_gas_price,
                    rpc_app
                        .tx_sender
                        .0
                        .sender_config
                        .vm_execution_cache_misses_limit,
                    rpc_app.tx_sender.vm_concurrency_limiter(),
                    rpc_app.tx_sender.storage_caches(),
                )
                .await
                .into_rpc(),
            )
            .expect("Can't merge debug namespace");
        }
        rpc
    }

    pub async fn build(
        mut self,
        stop_receiver: watch::Receiver<bool>,
    ) -> (Vec<tokio::task::JoinHandle<()>>, ReactiveHealthCheck) {
        if self.filters_limit.is_none() {
            vlog::warn!("Filters limit is not set - unlimited filters are allowed");
        }

        if self.namespaces.is_none() {
            vlog::warn!("debug_ API namespace will be disabled by default in ApiBuilder");
            self.namespaces = Some(Namespace::NON_DEBUG.to_vec());
        }

        if self
            .namespaces
            .as_ref()
            .unwrap()
            .contains(&Namespace::Pubsub)
            && matches!(&self.transport, Some(ApiTransport::Http(_)))
        {
            vlog::debug!("pubsub API is not supported for HTTP transport, ignoring");
        }

        match (&self.transport, self.subscriptions_limit) {
            (Some(ApiTransport::WebSocket(_)), None) => {
                vlog::warn!(
                    "`subscriptions_limit` is not set - unlimited subscriptions are allowed"
                );
            }
            (Some(ApiTransport::Http(_)), Some(_)) => {
                vlog::warn!(
                    "`subscriptions_limit` is ignored for HTTP transport, use WebSocket instead"
                );
            }
            _ => {}
        }

        match (self.backend, self.transport.take()) {
            (ApiBackend::Jsonrpc, Some(ApiTransport::Http(addr))) => {
                let (api_health_check, health_updater) = ReactiveHealthCheck::new("http_api");
                (
                    vec![
                        self.build_jsonrpc_http(addr, stop_receiver, health_updater)
                            .await,
                    ],
                    api_health_check,
                )
            }
            (ApiBackend::Jsonrpc, Some(ApiTransport::WebSocket(addr))) => {
                let (api_health_check, health_updater) = ReactiveHealthCheck::new("ws_api");
                (
                    self.build_jsonrpc_ws(addr, stop_receiver, health_updater)
                        .await,
                    api_health_check,
                )
            }
            (ApiBackend::Jsonrpsee, Some(ApiTransport::Http(addr))) => {
                let (api_health_check, health_updater) = ReactiveHealthCheck::new("http_api");
                (
                    vec![
                        self.build_jsonrpsee_http(addr, stop_receiver, health_updater)
                            .await,
                    ],
                    api_health_check,
                )
            }
            (ApiBackend::Jsonrpsee, Some(ApiTransport::WebSocket(addr))) => {
                let (api_health_check, health_updater) = ReactiveHealthCheck::new("ws_api");
                (
                    vec![
                        self.build_jsonrpsee_ws(addr, stop_receiver, health_updater)
                            .await,
                    ],
                    api_health_check,
                )
            }
            (_, None) => panic!("ApiTransport is not specified"),
        }
    }

    async fn build_jsonrpc_http(
        self,
        addr: SocketAddr,
        mut stop_receiver: watch::Receiver<bool>,
        health_updater: HealthUpdater,
    ) -> tokio::task::JoinHandle<()> {
        if self.batch_request_size_limit.is_some() {
            vlog::info!("`batch_request_size_limit` is not supported for `jsonrpc` backend, this value is ignored");
        }
        if self.response_body_size_limit.is_some() {
            vlog::info!("`response_body_size_limit` is not supported for `jsonrpc` backend, this value is ignored");
        }

        let mut io_handler = IoHandler::new();
        self.extend_jsonrpc_methods(&mut io_handler).await;
        let vm_barrier = self.vm_barrier.unwrap();

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("jsonrpc-http-worker")
            .worker_threads(self.threads.unwrap())
            .build()
            .unwrap();

        tokio::task::spawn_blocking(move || {
            let server = jsonrpc_http_server::ServerBuilder::new(io_handler)
                .threads(1)
                .event_loop_executor(runtime.handle().clone())
                .start_http(&addr)
                .unwrap();

            let close_handle = server.close_handle();
            let closing_vm_barrier = vm_barrier.clone();
            runtime.handle().spawn(async move {
                if stop_receiver.changed().await.is_ok() {
                    vlog::info!("Stop signal received, HTTP JSON-RPC server is shutting down");
                    closing_vm_barrier.close();
                    close_handle.close();
                }
            });

            health_updater.update(HealthStatus::Ready.into());
            server.wait();
            drop(health_updater);
            vlog::info!("HTTP JSON-RPC server stopped");
            runtime.block_on(Self::wait_for_vm(vm_barrier, "HTTP"));
            runtime.shutdown_timeout(GRACEFUL_SHUTDOWN_TIMEOUT);
        })
    }

    async fn wait_for_vm(vm_barrier: VmConcurrencyBarrier, transport: &str) {
        let wait_for_vm =
            tokio::time::timeout(GRACEFUL_SHUTDOWN_TIMEOUT, vm_barrier.wait_until_stopped());
        if wait_for_vm.await.is_err() {
            vlog::warn!(
                "VM execution on {transport} JSON-RPC server didn't stop after {GRACEFUL_SHUTDOWN_TIMEOUT:?}; \
                 forcing shutdown anyway"
            );
        } else {
            vlog::info!("VM execution on {transport} JSON-RPC server stopped");
        }
    }

    async fn extend_jsonrpc_methods<T>(&self, io: &mut MetaIoHandler<T>)
    where
        T: jsonrpc_core::Metadata,
    {
        let zksync_network_id = self.config.l2_chain_id;
        let rpc_state = self.build_rpc_state();
        let namespaces = self.namespaces.as_ref().unwrap();
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
            let hashes = BaseSystemContractsHashes {
                default_aa: rpc_state.tx_sender.0.sender_config.default_aa,
                bootloader: rpc_state.tx_sender.0.sender_config.bootloader,
            };
            let debug_ns = DebugNamespace::new(
                rpc_state.connection_pool,
                hashes,
                rpc_state.tx_sender.0.sender_config.fair_l2_gas_price,
                rpc_state
                    .tx_sender
                    .0
                    .sender_config
                    .vm_execution_cache_misses_limit,
                rpc_state.tx_sender.vm_concurrency_limiter(),
                rpc_state.tx_sender.storage_caches(),
            )
            .await;
            io.extend_with(debug_ns.to_delegate());
        }
    }

    async fn build_jsonrpc_ws(
        self,
        addr: SocketAddr,
        mut stop_receiver: watch::Receiver<bool>,
        health_updater: HealthUpdater,
    ) -> Vec<tokio::task::JoinHandle<()>> {
        if self.batch_request_size_limit.is_some() {
            vlog::info!("`batch_request_size_limit` is not supported for `jsonrpc` backend, this value is ignored");
        }
        if self.response_body_size_limit.is_some() {
            vlog::info!("`response_body_size_limit` is not supported for `jsonrpc` backend, this value is ignored");
        }

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("jsonrpc-ws-worker")
            .worker_threads(self.threads.unwrap())
            .build()
            .unwrap();

        let mut io_handler = PubSubHandler::default();
        let mut notify_handles = Vec::new();

        if self
            .namespaces
            .as_ref()
            .unwrap()
            .contains(&Namespace::Pubsub)
        {
            let pub_sub = EthSubscribe::new(runtime.handle().clone());
            let polling_interval = self.polling_interval.expect("Polling interval is not set");
            notify_handles.extend([
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

        let max_connections = self.subscriptions_limit.unwrap_or(usize::MAX);
        let vm_barrier = self.vm_barrier.unwrap();
        let server_handle = tokio::task::spawn_blocking(move || {
            let server = jsonrpc_ws_server::ServerBuilder::with_meta_extractor(
                io_handler,
                |context: &jsonrpc_ws_server::RequestContext| {
                    Arc::new(jsonrpc_pubsub::Session::new(context.sender()))
                },
            )
            .event_loop_executor(runtime.handle().clone())
            .max_connections(max_connections)
            .session_stats(TrackOpenWsConnections)
            .start(&addr)
            .unwrap();

            let close_handle = server.close_handle();
            let closing_vm_barrier = vm_barrier.clone();
            runtime.handle().spawn(async move {
                if stop_receiver.changed().await.is_ok() {
                    vlog::info!("Stop signal received, WS JSON-RPC server is shutting down");
                    closing_vm_barrier.close();
                    close_handle.close();
                }
            });

            health_updater.update(HealthStatus::Ready.into());
            server.wait().unwrap();
            drop(health_updater);
            vlog::info!("WS JSON-RPC server stopped");
            runtime.block_on(Self::wait_for_vm(vm_barrier, "WS"));
            runtime.shutdown_timeout(GRACEFUL_SHUTDOWN_TIMEOUT);
        });

        notify_handles.push(server_handle);
        notify_handles
    }

    async fn build_jsonrpsee_http(
        self,
        addr: SocketAddr,
        stop_receiver: watch::Receiver<bool>,
        health_updater: HealthUpdater,
    ) -> tokio::task::JoinHandle<()> {
        let rpc = self.build_rpc_module().await;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("jsonrpsee-http-worker")
            .worker_threads(self.threads.unwrap())
            .build()
            .unwrap();
        let vm_barrier = self.vm_barrier.unwrap();
        let batch_request_config = if let Some(limit) = self.batch_request_size_limit {
            BatchRequestConfig::Limit(limit as u32)
        } else {
            BatchRequestConfig::Unlimited
        };
        let response_body_size_limit = self
            .response_body_size_limit
            .map(|limit| limit as u32)
            .unwrap_or(u32::MAX);

        // Start the server in a separate tokio runtime from a dedicated thread.
        tokio::task::spawn_blocking(move || {
            runtime.block_on(Self::run_jsonrpsee_server(
                true,
                rpc,
                addr,
                stop_receiver,
                health_updater,
                vm_barrier,
                batch_request_config,
                response_body_size_limit,
            ));
            runtime.shutdown_timeout(GRACEFUL_SHUTDOWN_TIMEOUT);
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_jsonrpsee_server(
        is_http: bool,
        rpc: RpcModule<()>,
        addr: SocketAddr,
        mut stop_receiver: watch::Receiver<bool>,
        health_updater: HealthUpdater,
        vm_barrier: VmConcurrencyBarrier,
        batch_request_config: BatchRequestConfig,
        response_body_size_limit: u32,
    ) {
        let transport = if is_http { "HTTP" } else { "WS" };
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
        tokio::spawn(counter.run_emitter(Duration::from_secs(10), move |count| {
            metrics::histogram!("api.web3.in_flight_requests", count as f64, "scheme" => transport);
            future::ready(())
        }));
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
            .unwrap_or_else(|err| {
                panic!("Failed building {} JSON-RPC server: {}", transport, err);
            });
        let server_handle = server.start(rpc);

        let close_handle = server_handle.clone();
        let closing_vm_barrier = vm_barrier.clone();
        tokio::spawn(async move {
            if stop_receiver.changed().await.is_ok() {
                vlog::info!("Stop signal received, {transport} JSON-RPC server is shutting down");
                closing_vm_barrier.close();
                close_handle.stop().ok();
            }
        });
        health_updater.update(HealthStatus::Ready.into());

        server_handle.stopped().await;
        drop(health_updater);
        vlog::info!("{transport} JSON-RPC server stopped");
        Self::wait_for_vm(vm_barrier, transport).await;
    }

    async fn build_jsonrpsee_ws(
        self,
        addr: SocketAddr,
        stop_receiver: watch::Receiver<bool>,
        health_updater: HealthUpdater,
    ) -> tokio::task::JoinHandle<()> {
        vlog::warn!(
            "`eth_subscribe` is not implemented for jsonrpsee backend, use jsonrpc instead"
        );

        let rpc = self.build_rpc_module().await;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("jsonrpsee-ws-worker")
            .worker_threads(self.threads.unwrap())
            .build()
            .unwrap();
        let vm_barrier = self.vm_barrier.unwrap();

        let batch_request_config = if let Some(limit) = self.batch_request_size_limit {
            BatchRequestConfig::Limit(limit as u32)
        } else {
            BatchRequestConfig::Unlimited
        };
        let response_body_size_limit = self
            .response_body_size_limit
            .map(|limit| limit as u32)
            .unwrap_or(u32::MAX);

        // Start the server in a separate tokio runtime from a dedicated thread.
        tokio::task::spawn_blocking(move || {
            runtime.block_on(Self::run_jsonrpsee_server(
                false,
                rpc,
                addr,
                stop_receiver,
                health_updater,
                vm_barrier,
                batch_request_config,
                response_body_size_limit,
            ));
            runtime.shutdown_timeout(GRACEFUL_SHUTDOWN_TIMEOUT);
        })
    }
}

struct TrackOpenWsConnections;

impl jsonrpc_ws_server::SessionStats for TrackOpenWsConnections {
    fn open_session(&self, _id: jsonrpc_ws_server::SessionId) {
        metrics::increment_gauge!("api.ws.open_sessions", 1.0);
    }

    fn close_session(&self, _id: jsonrpc_ws_server::SessionId) {
        metrics::decrement_gauge!("api.ws.open_sessions", 1.0);
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
