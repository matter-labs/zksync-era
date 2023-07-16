// Built-in uses
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

// External uses
use futures::channel::oneshot;
use futures::FutureExt;
use jsonrpc_core::IoHandler;
use jsonrpc_http_server::hyper;
use jsonrpc_pubsub::PubSubHandler;
use tokio::sync::{watch, RwLock};
use tower_http::{cors::CorsLayer, metrics::InFlightRequestsLayer};

// Workspace uses
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_eth_signer::{EthereumSigner, PrivateKeySigner};
use zksync_types::{api, Address, MiniblockNumber, H256};
use zksync_web3_decl::{
    error::Web3Error,
    jsonrpsee::{server::ServerBuilder, RpcModule},
    namespaces::{
        DebugNamespaceServer, EnNamespaceServer, EthNamespaceServer, NetNamespaceServer,
        Web3NamespaceServer, ZksNamespaceServer,
    },
};

use self::state::InternalApiConfig;
use crate::l1_gas_price::L1GasPriceProvider;
use crate::sync_layer::SyncState;

// Local uses
use super::tx_sender::TxSender;
use crate::api_server::web3::api_health_check::ApiHealthCheck;
use backend_jsonrpc::{
    error::internal_error,
    namespaces::{
        debug::DebugNamespaceT, en::EnNamespaceT, eth::EthNamespaceT, net::NetNamespaceT,
        web3::Web3NamespaceT, zks::ZksNamespaceT,
    },
    pub_sub::Web3PubSub,
};
use namespaces::{
    DebugNamespace, EnNamespace, EthNamespace, EthSubscribe, NetNamespace, Web3Namespace,
    ZksNamespace,
};
use pubsub_notifier::{notify_blocks, notify_logs, notify_txs};
use state::{Filters, RpcState};
use zksync_health_check::CheckHealthStatus;

pub mod api_health_check;
pub mod backend_jsonrpc;
pub mod backend_jsonrpsee;
pub mod namespaces;
mod pubsub_notifier;
pub mod state;

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

#[derive(Debug)]
pub struct ApiBuilder<G> {
    backend: ApiBackend,
    pool: ConnectionPool,
    config: InternalApiConfig,
    transport: Option<ApiTransport>,
    tx_sender: Option<TxSender<G>>,
    filters_limit: Option<usize>,
    subscriptions_limit: Option<usize>,
    sync_state: Option<SyncState>,
    threads: Option<usize>,
    vm_concurrency_limit: Option<usize>,
    polling_interval: Option<Duration>,
    accounts: HashMap<Address, PrivateKeySigner>,
    debug_namespace_config: Option<(BaseSystemContractsHashes, u64, Option<usize>)>,
}

impl<G> ApiBuilder<G> {
    pub fn jsonrpsee_backend(config: InternalApiConfig, pool: ConnectionPool) -> Self {
        Self {
            backend: ApiBackend::Jsonrpsee,
            transport: None,
            pool,
            sync_state: None,
            tx_sender: None,
            filters_limit: None,
            subscriptions_limit: None,
            threads: None,
            vm_concurrency_limit: None,
            polling_interval: None,
            debug_namespace_config: None,
            accounts: Default::default(),
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
            filters_limit: None,
            subscriptions_limit: None,
            threads: None,
            vm_concurrency_limit: None,
            polling_interval: None,
            debug_namespace_config: None,
            accounts: Default::default(),
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

    pub fn with_tx_sender(mut self, tx_sender: TxSender<G>) -> Self {
        self.tx_sender = Some(tx_sender);
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

    pub fn enable_debug_namespace(
        mut self,
        base_system_contract_hashes: BaseSystemContractsHashes,
        fair_l2_gas_price: u64,
        cache_misses_limit: Option<usize>,
    ) -> Self {
        self.debug_namespace_config = Some((
            base_system_contract_hashes,
            fair_l2_gas_price,
            cache_misses_limit,
        ));
        self
    }

    pub fn enable_oz_tests(mut self, account_pks: Vec<H256>) -> Self {
        if cfg!(feature = "openzeppelin_tests") {
            self.accounts = account_pks
                .into_iter()
                .map(|pk| {
                    let signer = PrivateKeySigner::new(pk);
                    let address = futures::executor::block_on(signer.get_address())
                        .expect("Failed to get address of a signer");
                    (address, signer)
                })
                .collect();
        } else {
            vlog::info!("OpenZeppelin tests are not enabled, ignoring `enable_oz_tests` call");
        }
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
            accounts: self.accounts.clone(),
            #[cfg(feature = "openzeppelin_tests")]
            known_bytecodes: Arc::new(RwLock::new(Default::default())),
        }
    }

    async fn build_rpc_module(&self) -> RpcModule<EthNamespace<G>> {
        let zksync_network_id = self.config.l2_chain_id;
        let rpc_app = self.build_rpc_state();

        // Declare namespaces we have.
        let eth = EthNamespace::new(rpc_app.clone());
        let net = NetNamespace::new(zksync_network_id);
        let web3 = Web3Namespace;
        let zks = ZksNamespace::new(rpc_app.clone());
        let en = EnNamespace::new(rpc_app.clone());

        // Collect all the methods into a single RPC module.
        let mut rpc: RpcModule<_> = eth.into_rpc();
        rpc.merge(net.into_rpc())
            .expect("Can't merge net namespace");
        rpc.merge(web3.into_rpc())
            .expect("Can't merge web3 namespace");
        rpc.merge(zks.into_rpc())
            .expect("Can't merge zks namespace");
        rpc.merge(en.into_rpc()).expect("Can't merge en namespace");

        if let Some((hashes, fair_l2_gas_price, cache_misses_limit)) = self.debug_namespace_config {
            rpc.merge(
                DebugNamespace::new(
                    rpc_app.connection_pool,
                    hashes,
                    fair_l2_gas_price,
                    cache_misses_limit,
                    rpc_app.tx_sender.0.vm_concurrency_limiter.clone(),
                    rpc_app.tx_sender.0.factory_deps_cache.clone(),
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
    ) -> (Vec<tokio::task::JoinHandle<()>>, ApiHealthCheck) {
        if self.filters_limit.is_none() {
            vlog::warn!("Filters limit is not set - unlimited filters are allowed");
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
                let (api_health_check, status_sender) = self.create_health_check();
                (
                    vec![
                        self.build_jsonrpc_http(addr, stop_receiver, status_sender)
                            .await,
                    ],
                    api_health_check,
                )
            }
            (ApiBackend::Jsonrpc, Some(ApiTransport::WebSocket(addr))) => {
                let (api_health_check, status_sender) = self.create_health_check();
                (
                    self.build_jsonrpc_ws(addr, stop_receiver, status_sender),
                    api_health_check,
                )
            }
            (ApiBackend::Jsonrpsee, Some(ApiTransport::Http(addr))) => {
                let (api_health_check, status_sender) = self.create_health_check();
                (
                    vec![
                        self.build_jsonrpsee_http(addr, stop_receiver, status_sender)
                            .await,
                    ],
                    api_health_check,
                )
            }
            (ApiBackend::Jsonrpsee, Some(ApiTransport::WebSocket(addr))) => {
                let (api_health_check, status_sender) = self.create_health_check();
                (
                    vec![
                        self.build_jsonrpsee_ws(addr, stop_receiver, status_sender)
                            .await,
                    ],
                    api_health_check,
                )
            }
            (_, None) => panic!("ApiTransport is not specified"),
        }
    }

    fn create_health_check(&self) -> (ApiHealthCheck, watch::Sender<CheckHealthStatus>) {
        let (status_sender, receiver) =
            watch::channel(CheckHealthStatus::NotReady("Api is not ready".into()));
        (ApiHealthCheck::new(receiver), status_sender)
    }

    async fn build_jsonrpc_http(
        self,
        addr: SocketAddr,
        mut stop_receiver: watch::Receiver<bool>,
        api_health_check: watch::Sender<CheckHealthStatus>,
    ) -> tokio::task::JoinHandle<()> {
        let io_handler = {
            let zksync_network_id = self.config.l2_chain_id;
            let rpc_state = self.build_rpc_state();
            let mut io = IoHandler::new();
            io.extend_with(EthNamespace::new(rpc_state.clone()).to_delegate());
            io.extend_with(ZksNamespace::new(rpc_state.clone()).to_delegate());
            io.extend_with(EnNamespace::new(rpc_state.clone()).to_delegate());
            io.extend_with(Web3Namespace.to_delegate());
            io.extend_with(NetNamespace::new(zksync_network_id).to_delegate());
            if let Some((hashes, fair_l2_gas_price, cache_misses_limit)) =
                self.debug_namespace_config
            {
                io.extend_with(
                    DebugNamespace::new(
                        rpc_state.connection_pool,
                        hashes,
                        fair_l2_gas_price,
                        cache_misses_limit,
                        rpc_state.tx_sender.0.vm_concurrency_limiter.clone(),
                        rpc_state.tx_sender.0.factory_deps_cache.clone(),
                    )
                    .await
                    .to_delegate(),
                );
            }

            io
        };

        let (sender, recv) = oneshot::channel::<()>();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(self.threads.unwrap())
                .build()
                .unwrap();

            let server = jsonrpc_http_server::ServerBuilder::new(io_handler)
                .threads(1)
                .event_loop_executor(runtime.handle().clone())
                .start_http(&addr)
                .unwrap();

            let close_handler = server.close_handle();
            std::thread::spawn(move || {
                let stop_signal = futures::executor::block_on(stop_receiver.changed());
                if stop_signal.is_ok() {
                    vlog::info!("Stop signal received, web3 HTTP JSON RPC API is shutting down");
                    close_handler.close();
                }
            });
            api_health_check.send(CheckHealthStatus::Ready).unwrap();
            server.wait();
            runtime.shutdown_timeout(Duration::from_secs(10));

            let _ = sender;
        });

        tokio::spawn(recv.map(drop))
    }

    async fn build_jsonrpsee_http(
        self,
        addr: SocketAddr,
        mut stop_receiver: watch::Receiver<bool>,
        api_health_check: watch::Sender<CheckHealthStatus>,
    ) -> tokio::task::JoinHandle<()> {
        let rpc = self.build_rpc_module().await;

        // Start the server in a separate tokio runtime from a dedicated thread.
        let (sender, recv) = oneshot::channel::<()>();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(self.threads.unwrap())
                .build()
                .unwrap();

            // Setup CORS.
            let cors = CorsLayer::new()
                // Allow `POST` when accessing the resource
                .allow_methods([hyper::Method::POST])
                // Allow requests from any origin
                .allow_origin(tower_http::cors::Any)
                .allow_headers([hyper::header::CONTENT_TYPE]);

            // Setup metrics for the number of in-flight txs.
            let (in_flight_requests_layer, counter) = InFlightRequestsLayer::pair();
            runtime.spawn(counter.run_emitter(Duration::from_secs(10), |count| async move {
                metrics::histogram!("api.web3.in_flight_requests", count as f64, "scheme" => "http");
            }));

            // Prepare middleware.
            let middleware = tower::ServiceBuilder::new()
                .layer(in_flight_requests_layer)
                .layer(cors);

            runtime.block_on(async move {
                let server = ServerBuilder::default()
                    .http_only()
                    .max_connections(5000)
                    .set_middleware(middleware)
                    .build(addr)
                    .await
                    .expect("Can't start the HTTP JSON RPC server");

                let server_handle = server
                    .start(rpc)
                    .expect("Failed to start HTTP JSON RPC application");

                let close_handle = server_handle.clone();
                tokio::spawn(async move {
                    if stop_receiver.changed().await.is_ok() {
                        vlog::info!(
                            "Stop signal received, web3 HTTP JSON RPC API is shutting down"
                        );
                        close_handle.stop().unwrap();
                    }
                });
                api_health_check.send(CheckHealthStatus::Ready).unwrap();
                server_handle.stopped().await;
                vlog::info!("HTTP JSON RPC API stopped");
            });
            runtime.shutdown_timeout(Duration::from_secs(10));
            sender.send(()).unwrap();
        });

        // Notifier for the rest of application about the end of the task.
        tokio::spawn(recv.map(drop))
    }

    async fn build_jsonrpsee_ws(
        self,
        addr: SocketAddr,
        mut stop_receiver: watch::Receiver<bool>,
        api_health_check: watch::Sender<CheckHealthStatus>,
    ) -> tokio::task::JoinHandle<()> {
        vlog::warn!(
            "`eth_subscribe` is not implemented for jsonrpsee backend, use jsonrpc instead"
        );

        let rpc = self.build_rpc_module().await;

        // Start the server in a separate tokio runtime from a dedicated thread.
        let (sender, recv) = oneshot::channel::<()>();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(self.threads.unwrap())
                .build()
                .unwrap();

            runtime.block_on(async move {
                let server = ServerBuilder::default()
                    .ws_only()
                    .build(addr)
                    .await
                    .expect("Can't start the WS JSON RPC server");

                let server_handle = server
                    .start(rpc)
                    .expect("Failed to start WS JSON RPC application");

                api_health_check.send(CheckHealthStatus::Ready).unwrap();
                let close_handle = server_handle.clone();
                tokio::spawn(async move {
                    if stop_receiver.changed().await.is_ok() {
                        vlog::info!("Stop signal received, web3 WS JSON RPC API is shutting down");
                        close_handle.stop().unwrap();
                    }
                });
                server_handle.stopped().await;
            });
            runtime.shutdown_timeout(Duration::from_secs(10));
            sender.send(()).unwrap();
        });

        // Notifier for the rest of application about the end of the task.
        tokio::spawn(recv.map(drop))
    }

    fn build_jsonrpc_ws(
        self,
        addr: SocketAddr,
        mut stop_receiver: watch::Receiver<bool>,
        api_health_check: watch::Sender<CheckHealthStatus>,
    ) -> Vec<tokio::task::JoinHandle<()>> {
        let jsonrpc_runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(self.threads.unwrap())
            .build()
            .unwrap();

        let pub_sub = EthSubscribe::new(jsonrpc_runtime.handle().clone());
        let polling_interval = self.polling_interval.expect("Polling interval is not set");

        let mut notify_handles = vec![
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
        ];

        let (sender, recv) = oneshot::channel::<()>();
        let io = {
            let zksync_network_id = self.config.l2_chain_id;
            let rpc_state = self.build_rpc_state();
            let mut io = PubSubHandler::default();
            io.extend_with(pub_sub.to_delegate());
            io.extend_with(EthNamespace::new(rpc_state.clone()).to_delegate());
            io.extend_with(ZksNamespace::new(rpc_state.clone()).to_delegate());
            io.extend_with(EnNamespace::new(rpc_state).to_delegate());
            io.extend_with(Web3Namespace.to_delegate());
            io.extend_with(NetNamespace::new(zksync_network_id).to_delegate());
            io
        };

        std::thread::spawn(move || {
            let server = jsonrpc_ws_server::ServerBuilder::with_meta_extractor(
                io,
                |context: &jsonrpc_ws_server::RequestContext| {
                    Arc::new(jsonrpc_pubsub::Session::new(context.sender()))
                },
            )
            .event_loop_executor(jsonrpc_runtime.handle().clone())
            .max_connections(self.subscriptions_limit.unwrap_or(usize::MAX))
            .session_stats(TrackOpenWsConnections)
            .start(&addr)
            .unwrap();
            let close_handler = server.close_handle();

            std::thread::spawn(move || {
                let stop_signal = futures::executor::block_on(stop_receiver.changed());
                if stop_signal.is_ok() {
                    close_handler.close();
                    vlog::info!("Stop signal received, WS JSON RPC API is shutting down");
                }
            });

            api_health_check.send(CheckHealthStatus::Ready).unwrap();
            server.wait().unwrap();
            jsonrpc_runtime.shutdown_timeout(Duration::from_secs(10));
            let _ = sender;
        });

        notify_handles.push(tokio::spawn(recv.map(drop)));
        notify_handles
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
