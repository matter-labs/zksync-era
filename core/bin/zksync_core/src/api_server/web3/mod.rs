// Built-in uses
use std::sync::{Arc, RwLock};
use std::time::Duration;
// External uses
use futures::channel::oneshot;
use futures::FutureExt;
use jsonrpc_core::IoHandler;
use jsonrpc_pubsub::PubSubHandler;
use once_cell::{self, sync::Lazy};
use tokio::sync::watch;
use zksync_dal::ConnectionPool;

// Workspace uses
use zksync_config::ZkSyncConfig;
use zksync_eth_client::clients::http_client::EthereumClient;
use zksync_eth_signer::{EthereumSigner, PrivateKeySigner};
use zksync_types::H256;
use zksync_web3_decl::{
    jsonrpsee::{server::ServerBuilder, RpcModule},
    namespaces::{EthNamespaceServer, NetNamespaceServer, Web3NamespaceServer, ZksNamespaceServer},
};

use crate::gas_adjuster::GasAdjuster;

// Local uses
use super::tx_sender::TxSender;
use backend_jsonrpc::{
    namespaces::{
        eth::EthNamespaceT, net::NetNamespaceT, web3::Web3NamespaceT, zks::ZksNamespaceT,
    },
    pub_sub::Web3PubSub,
};
use namespaces::{EthNamespace, EthSubscribe, NetNamespace, Web3Namespace, ZksNamespace};
use pubsub_notifier::{notify_blocks, notify_logs, notify_txs};
use state::{Filters, RpcState};
use zksync_contracts::{ESTIMATE_FEE_BLOCK_CODE, PLAYGROUND_BLOCK_BOOTLOADER_CODE};

pub mod backend_jsonrpc;
pub mod backend_jsonrpsee;
pub mod namespaces;
mod pubsub_notifier;
pub mod state;

pub fn get_config() -> &'static ZkSyncConfig {
    static ZKSYNC_CONFIG: Lazy<ZkSyncConfig> = Lazy::new(ZkSyncConfig::from_env);

    &ZKSYNC_CONFIG
}

impl RpcState {
    pub fn init(
        master_connection_pool: ConnectionPool,
        replica_connection_pool: ConnectionPool,
        req_entities_limit: usize,
        filters_limit: usize,
        account_pks: Vec<H256>,
        gas_adjuster: Arc<GasAdjuster<EthereumClient>>,
    ) -> Self {
        let config = get_config();
        let mut storage = replica_connection_pool.access_storage_blocking();

        let base_system_contracts = storage.storage_dal().get_base_system_contracts(
            config.chain.state_keeper.bootloader_hash,
            config.chain.state_keeper.default_aa_hash,
        );

        let mut playground_base_system_contracts = base_system_contracts.clone();
        let mut estimate_fee_base_system_contracts = base_system_contracts;
        playground_base_system_contracts.bootloader = PLAYGROUND_BLOCK_BOOTLOADER_CODE.clone();
        estimate_fee_base_system_contracts.bootloader = ESTIMATE_FEE_BLOCK_CODE.clone();

        drop(storage);
        let tx_sender = TxSender::new(
            config,
            master_connection_pool,
            replica_connection_pool.clone(),
            gas_adjuster,
            playground_base_system_contracts,
            estimate_fee_base_system_contracts,
        );

        let accounts = if cfg!(feature = "openzeppelin_tests") {
            account_pks
                .into_iter()
                .map(|pk| {
                    let signer = PrivateKeySigner::new(pk);
                    let address = futures::executor::block_on(signer.get_address())
                        .expect("Failed to get address of a signer");
                    (address, signer)
                })
                .collect()
        } else {
            Default::default()
        };

        RpcState {
            installed_filters: Arc::new(RwLock::new(Filters::new(filters_limit))),
            connection_pool: replica_connection_pool,
            tx_sender,
            req_entities_limit,
            accounts,
            config,
            #[cfg(feature = "openzeppelin_tests")]
            known_bytecodes: Arc::new(RwLock::new(Default::default())),
        }
    }
}

pub fn start_http_rpc_server_old(
    master_connection_pool: ConnectionPool,
    replica_connection_pool: ConnectionPool,
    config: &ZkSyncConfig,
    _stop_receiver: watch::Receiver<bool>,
    gas_adjuster: Arc<GasAdjuster<EthereumClient>>,
) -> tokio::task::JoinHandle<()> {
    let io_handler = build_http_io_handler(
        master_connection_pool,
        replica_connection_pool,
        config,
        gas_adjuster,
    );
    let addr = config.api.web3_json_rpc.http_bind_addr();
    let threads_per_server = config.api.web3_json_rpc.threads_per_server as usize;

    let (sender, recv) = oneshot::channel::<()>();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(threads_per_server)
            .build()
            .unwrap();

        let server = jsonrpc_http_server::ServerBuilder::new(io_handler)
            .threads(1)
            .event_loop_executor(runtime.handle().clone())
            .start_http(&addr)
            .unwrap();

        server.wait();
        let _ = sender;
    });

    tokio::spawn(recv.map(drop))
}

fn start_notifying_active_subs(
    pub_sub: EthSubscribe,
    connection_pool: ConnectionPool,
    polling_interval: Duration,
    stop_receiver: watch::Receiver<bool>,
) -> Vec<tokio::task::JoinHandle<()>> {
    vec![
        tokio::spawn(notify_blocks(
            pub_sub.active_block_subs,
            connection_pool.clone(),
            polling_interval,
            stop_receiver.clone(),
        )),
        tokio::spawn(notify_txs(
            pub_sub.active_tx_subs,
            connection_pool.clone(),
            polling_interval,
            stop_receiver.clone(),
        )),
        tokio::spawn(notify_logs(
            pub_sub.active_log_subs,
            connection_pool,
            polling_interval,
            stop_receiver,
        )),
    ]
}

pub fn start_ws_rpc_server_old(
    master_connection_pool: ConnectionPool,
    replica_connection_pool: ConnectionPool,
    config: &ZkSyncConfig,
    stop_receiver: watch::Receiver<bool>,
    gas_adjuster: Arc<GasAdjuster<EthereumClient>>,
) -> Vec<tokio::task::JoinHandle<()>> {
    let pub_sub = EthSubscribe::default();
    let mut notify_handles = start_notifying_active_subs(
        pub_sub.clone(),
        replica_connection_pool.clone(),
        config.api.web3_json_rpc.pubsub_interval(),
        stop_receiver.clone(),
    );

    let addr = config.api.web3_json_rpc.ws_bind_addr();
    let (sender, recv) = oneshot::channel::<()>();
    let io = build_pubsub_io_handler(
        master_connection_pool,
        replica_connection_pool,
        config,
        gas_adjuster.clone(),
        pub_sub,
    );

    let server = jsonrpc_ws_server::ServerBuilder::with_meta_extractor(
        io,
        |context: &jsonrpc_ws_server::RequestContext| {
            Arc::new(jsonrpc_pubsub::Session::new(context.sender()))
        },
    )
    .max_connections(config.api.web3_json_rpc.subscriptions_limit())
    .start(&addr)
    .unwrap();
    let close_handler = server.close_handle();

    std::thread::spawn(move || {
        server.wait().unwrap();
        let _ = sender;
    });
    let mut thread_stop_receiver = stop_receiver.clone();
    std::thread::spawn(move || {
        let stop_signal = futures::executor::block_on(thread_stop_receiver.changed());
        if stop_signal.is_ok() {
            close_handler.close();
            vlog::info!("Stop signal received, WS JSON RPC API is shutting down");
        }
    });

    notify_handles.push(tokio::spawn(gas_adjuster.run(stop_receiver)));
    notify_handles.push(tokio::spawn(recv.map(drop)));
    notify_handles
}

pub fn start_http_rpc_server(
    master_connection_pool: ConnectionPool,
    replica_connection_pool: ConnectionPool,
    config: &ZkSyncConfig,
    gas_adjuster: Arc<GasAdjuster<EthereumClient>>,
) -> tokio::task::JoinHandle<()> {
    let rpc = build_rpc_module(
        master_connection_pool,
        replica_connection_pool,
        config,
        gas_adjuster,
    );
    let addr = config.api.web3_json_rpc.http_bind_addr();
    let threads_per_server = config.api.web3_json_rpc.threads_per_server as usize;

    // Start the server in a separate tokio runtime from a dedicated thread.
    let (sender, recv) = oneshot::channel::<()>();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(threads_per_server)
            .build()
            .unwrap();

        runtime.block_on(async move {
            let server = ServerBuilder::default()
                .http_only()
                .max_connections(5000)
                .build(addr)
                .await
                .expect("Can't start the HTTP JSON RPC server");

            let server_handle = server
                .start(rpc)
                .expect("Failed to start HTTP JSON RPC application");
            server_handle.stopped().await
        });

        sender.send(()).unwrap();
    });

    // Notifier for the rest of application about the end of the task.
    tokio::spawn(recv.map(drop))
}

pub fn start_ws_rpc_server(
    master_connection_pool: ConnectionPool,
    replica_connection_pool: ConnectionPool,
    config: &ZkSyncConfig,
    gas_adjuster: Arc<GasAdjuster<EthereumClient>>,
) -> tokio::task::JoinHandle<()> {
    let rpc = build_rpc_module(
        master_connection_pool,
        replica_connection_pool,
        config,
        gas_adjuster,
    );
    let addr = config.api.web3_json_rpc.ws_bind_addr();
    let threads_per_server = config.api.web3_json_rpc.threads_per_server as usize;

    // Start the server in a separate tokio runtime from a dedicated thread.
    let (sender, recv) = oneshot::channel::<()>();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(threads_per_server)
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
            server_handle.stopped().await
        });

        sender.send(()).unwrap();
    });

    // Notifier for the rest of application about the end of the task.
    tokio::spawn(recv.map(drop))
}

fn build_rpc_state(
    master_connection_pool: ConnectionPool,
    replica_connection_pool: ConnectionPool,
    config: &ZkSyncConfig,
    gas_adjuster: Arc<GasAdjuster<EthereumClient>>,
) -> RpcState {
    let req_entities_limit = config.api.web3_json_rpc.req_entities_limit();
    let filters_limit = config.api.web3_json_rpc.filters_limit();
    let account_pks = config.api.web3_json_rpc.account_pks();

    RpcState::init(
        master_connection_pool,
        replica_connection_pool,
        req_entities_limit,
        filters_limit,
        account_pks,
        gas_adjuster,
    )
}

fn build_http_io_handler(
    master_connection_pool: ConnectionPool,
    replica_connection_pool: ConnectionPool,
    config: &ZkSyncConfig,
    gas_adjuster: Arc<GasAdjuster<EthereumClient>>,
) -> IoHandler {
    let rpc_state = build_rpc_state(
        master_connection_pool,
        replica_connection_pool,
        config,
        gas_adjuster,
    );
    let mut io = IoHandler::new();
    io.extend_with(EthNamespace::new(rpc_state.clone()).to_delegate());
    io.extend_with(ZksNamespace::new(rpc_state).to_delegate());
    io.extend_with(Web3Namespace.to_delegate());
    io.extend_with(NetNamespace.to_delegate());

    io
}

fn build_pubsub_io_handler(
    master_connection_pool: ConnectionPool,
    replica_connection_pool: ConnectionPool,
    config: &ZkSyncConfig,
    gas_adjuster: Arc<GasAdjuster<EthereumClient>>,
    pub_sub: EthSubscribe,
) -> PubSubHandler<Arc<jsonrpc_pubsub::Session>> {
    let rpc_state = build_rpc_state(
        master_connection_pool,
        replica_connection_pool,
        config,
        gas_adjuster,
    );
    let mut io = PubSubHandler::default();
    io.extend_with(pub_sub.to_delegate());
    io.extend_with(EthNamespace::new(rpc_state.clone()).to_delegate());
    io.extend_with(ZksNamespace::new(rpc_state).to_delegate());
    io.extend_with(Web3Namespace.to_delegate());
    io.extend_with(NetNamespace.to_delegate());

    io
}

fn build_rpc_module(
    master_connection_pool: ConnectionPool,
    replica_connection_pool: ConnectionPool,
    config: &ZkSyncConfig,
    gas_adjuster: Arc<GasAdjuster<EthereumClient>>,
) -> RpcModule<EthNamespace> {
    let rpc_app = build_rpc_state(
        master_connection_pool,
        replica_connection_pool,
        config,
        gas_adjuster,
    );

    // Declare namespaces we have.
    let eth = EthNamespace::new(rpc_app.clone());
    let net = NetNamespace;
    let web3 = Web3Namespace;
    let zks = ZksNamespace::new(rpc_app);

    // Collect all the methods into a single RPC module.
    let mut rpc: RpcModule<_> = eth.into_rpc();
    rpc.merge(net.into_rpc())
        .expect("Can't merge net namespace");
    rpc.merge(web3.into_rpc())
        .expect("Can't merge web3 namespace");
    rpc.merge(zks.into_rpc())
        .expect("Can't merge zks namespace");
    rpc
}
