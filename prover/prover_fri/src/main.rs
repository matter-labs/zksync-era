#![feature(generic_const_exprs)]

use tokio::sync::oneshot;
use tokio::sync::watch::Receiver;
use tokio::task::JoinHandle;

use zksync_config::configs::fri_prover_group::{CircuitIdRoundTuple, FriProverGroupConfig};
use zksync_config::configs::{FriProverConfig, PrometheusConfig};
use zksync_config::{ApiConfig, ObjectStoreConfig};
use zksync_dal::connection::DbVariant;
use zksync_dal::ConnectionPool;
use zksync_object_store::{ObjectStore, ObjectStoreFactory};

use zksync_queued_job_processor::JobProcessor;
use zksync_utils::wait_for_tasks::wait_for_tasks;

mod gpu_prover_job_processor;
mod prover_job_processor;
mod socket_listener;
mod utils;

#[tokio::main]
async fn main() {
    vlog::init();
    let sentry_guard = vlog::init_sentry();
    let prover_config = FriProverConfig::from_env();
    let prometheus_config = PrometheusConfig {
        listener_port: prover_config.prometheus_port,
        ..ApiConfig::from_env().prometheus
    };

    match sentry_guard {
        Some(_) => vlog::info!(
            "Starting Sentry url: {}",
            std::env::var("MISC_SENTRY_URL").unwrap(),
        ),
        None => vlog::info!("No sentry url configured"),
    }

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(sender) = stop_signal_sender.take() {
            sender.send(()).ok();
        }
    })
    .expect("Error setting Ctrl+C handler");

    let (stop_sender, stop_receiver) = tokio::sync::watch::channel(false);
    let blob_store = ObjectStoreFactory::prover_from_env();
    let public_blob_store = ObjectStoreFactory::new(ObjectStoreConfig::public_from_env())
        .create_store()
        .await;
    let specialized_group_id = prover_config.specialized_group_id;

    let circuit_ids_for_round_to_be_proven = FriProverGroupConfig::from_env()
        .get_circuit_ids_for_group_id(specialized_group_id)
        .unwrap_or(vec![]);

    vlog::info!(
        "Starting FRI proof generation for group: {} with circuits: {:?}",
        specialized_group_id,
        circuit_ids_for_round_to_be_proven.clone()
    );
    let pool = ConnectionPool::builder(DbVariant::Prover).build().await;

    let prover_tasks = get_prover_tasks(
        prover_config,
        stop_receiver,
        blob_store,
        public_blob_store,
        pool,
        circuit_ids_for_round_to_be_proven,
    )
    .await;

    let mut tasks = vec![prometheus_exporter::run_prometheus_exporter(
        prometheus_config.listener_port,
        None,
    )];
    tasks.extend(prover_tasks);

    let particular_crypto_alerts = None;
    let graceful_shutdown = None::<futures::future::Ready<()>>;
    let tasks_allowed_to_finish = false;
    tokio::select! {
        _ = wait_for_tasks(tasks, particular_crypto_alerts, graceful_shutdown, tasks_allowed_to_finish) => {},
        _ = stop_signal_receiver => {
            vlog::info!("Stop signal received, shutting down");
        },
    }

    stop_sender.send(true).ok();
}

#[cfg(not(feature = "gpu"))]
async fn get_prover_tasks(
    prover_config: FriProverConfig,
    stop_receiver: Receiver<bool>,
    store_factory: ObjectStoreFactory,
    public_blob_store: Box<dyn ObjectStore>,
    pool: ConnectionPool,
    circuit_ids_for_round_to_be_proven: Vec<CircuitIdRoundTuple>,
) -> Vec<JoinHandle<()>> {
    use crate::prover_job_processor::{load_setup_data_cache, Prover};

    let setup_load_mode = load_setup_data_cache(&prover_config);
    let prover = Prover::new(
        store_factory.create_store().await,
        public_blob_store,
        prover_config,
        pool,
        setup_load_mode,
        circuit_ids_for_round_to_be_proven,
    );
    vec![tokio::spawn(prover.run(stop_receiver, None))]
}

#[cfg(feature = "gpu")]
async fn get_prover_tasks(
    prover_config: FriProverConfig,
    stop_receiver: Receiver<bool>,
    store_factory: ObjectStoreFactory,
    public_blob_store: Box<dyn ObjectStore>,
    pool: ConnectionPool,
    circuit_ids_for_round_to_be_proven: Vec<CircuitIdRoundTuple>,
) -> Vec<JoinHandle<()>> {
    use gpu_prover_job_processor::gpu_prover;
    use local_ip_address::local_ip;
    use socket_listener::gpu_socket_listener;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use zksync_prover_fri_types::queue::FixedSizeQueue;
    use zksync_prover_utils::region_fetcher::get_zone;
    use zksync_types::proofs::SocketAddress;

    let setup_load_mode = gpu_prover::load_setup_data_cache(&prover_config);
    let witness_vector_queue = FixedSizeQueue::new(prover_config.queue_capacity);
    let shared_witness_vector_queue = Arc::new(Mutex::new(witness_vector_queue));
    let consumer = shared_witness_vector_queue.clone();
    let zone = get_zone().await;
    let local_ip = local_ip().expect("Failed obtaining local IP address");
    let address = SocketAddress {
        host: local_ip,
        port: prover_config.witness_vector_receiver_port,
    };
    let prover = gpu_prover::Prover::new(
        store_factory.create_store().await,
        public_blob_store,
        prover_config.clone(),
        pool.clone(),
        setup_load_mode,
        circuit_ids_for_round_to_be_proven.clone(),
        consumer,
        address.clone(),
        zone.clone(),
    );
    let producer = shared_witness_vector_queue.clone();

    vlog::info!(
        "local IP address is: {:?} in zone: {}",
        local_ip,
        zone.clone()
    );
    let socket_listener = gpu_socket_listener::SocketListener::new(
        address,
        producer,
        pool.clone(),
        prover_config.specialized_group_id,
        zone,
    );
    vec![
        tokio::spawn(socket_listener.listen_incoming_connections(stop_receiver.clone())),
        tokio::spawn(prover.run(stop_receiver, None)),
    ]
}
