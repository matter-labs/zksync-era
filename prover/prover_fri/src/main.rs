#![feature(generic_const_exprs)]
use anyhow::Context as _;
use std::future::Future;
use tokio::sync::oneshot;
use tokio::sync::watch::Receiver;
use tokio::task::JoinHandle;

use prometheus_exporter::PrometheusExporterConfig;
use zksync_config::configs::fri_prover_group::FriProverGroupConfig;
use zksync_config::configs::FriProverConfig;
use zksync_config::{FromEnv, ObjectStoreConfig};
use zksync_dal::connection::DbVariant;
use zksync_dal::ConnectionPool;
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_prover_fri_utils::get_all_circuit_id_round_tuples_for;

use local_ip_address::local_ip;
use zksync_prover_utils::region_fetcher::get_zone;
use zksync_queued_job_processor::JobProcessor;
use zksync_types::basic_fri_types::CircuitIdRoundTuple;
use zksync_types::proofs::GpuProverInstanceStatus;
use zksync_types::proofs::SocketAddress;
use zksync_utils::wait_for_tasks::wait_for_tasks;

mod gpu_prover_job_processor;
mod prover_job_processor;
mod socket_listener;
mod utils;

async fn graceful_shutdown(port: u16) -> anyhow::Result<impl Future<Output = ()>> {
    let pool = ConnectionPool::singleton(DbVariant::Prover)
        .build()
        .await
        .context("failed to build a connection pool")?;
    let host = local_ip().context("Failed obtaining local IP address")?;
    let zone = get_zone().await.context("get_zone()")?;
    let address = SocketAddress { host, port };
    Ok(async move {
        pool.access_storage()
            .await
            .unwrap()
            .fri_gpu_prover_queue_dal()
            .update_prover_instance_status(address, GpuProverInstanceStatus::Dead, zone)
            .await
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let log_format = vlog::log_format_from_env();
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let sentry_url = vlog::sentry_url_from_env();
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let environment = vlog::environment_from_env();

    let mut builder = vlog::ObservabilityBuilder::new().with_log_format(log_format);
    if let Some(sentry_url) = &sentry_url {
        builder = builder
            .with_sentry_url(sentry_url)
            .context("Invalid Sentry URL")?
            .with_sentry_environment(environment);
    }
    let _guard = builder.build();

    // Report whether sentry is running after the logging subsystem was initialized.
    if let Some(sentry_url) = sentry_url {
        tracing::info!("Sentry configured with URL: {sentry_url}",);
    } else {
        tracing::info!("No sentry URL was provided");
    }

    let prover_config = FriProverConfig::from_env().context("FriProverConfig::from_env()")?;
    let exporter_config = PrometheusExporterConfig::pull(prover_config.prometheus_port);

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(sender) = stop_signal_sender.take() {
            sender.send(()).ok();
        }
    })
    .context("Error setting Ctrl+C handler")?;

    let (stop_sender, stop_receiver) = tokio::sync::watch::channel(false);
    let blob_store =
        ObjectStoreFactory::prover_from_env().context("ObjectStoreFactory::prover_from_env()")?;
    let public_blob_store = match prover_config.shall_save_to_public_bucket {
        false => None,
        true => Some(
            ObjectStoreFactory::new(
                ObjectStoreConfig::public_from_env()
                    .context("ObjectStoreConfig::public_from_env()")?,
            )
            .create_store()
            .await,
        ),
    };
    let specialized_group_id = prover_config.specialized_group_id;

    let circuit_ids_for_round_to_be_proven = FriProverGroupConfig::from_env()
        .get_circuit_ids_for_group_id(specialized_group_id)
        .unwrap_or_default();
    let circuit_ids_for_round_to_be_proven =
        get_all_circuit_id_round_tuples_for(circuit_ids_for_round_to_be_proven);

    tracing::info!(
        "Starting FRI proof generation for group: {} with circuits: {:?}",
        specialized_group_id,
        circuit_ids_for_round_to_be_proven.clone()
    );
    let pool = ConnectionPool::builder(DbVariant::Prover)
        .build()
        .await
        .context("failed to build a connection pool")?;
    let port = prover_config.witness_vector_receiver_port;
    let prover_tasks = get_prover_tasks(
        prover_config,
        stop_receiver.clone(),
        blob_store,
        public_blob_store,
        pool,
        circuit_ids_for_round_to_be_proven,
    )
    .await
    .context("get_prover_tasks()")?;

    let mut tasks = vec![tokio::spawn(exporter_config.run(stop_receiver))];
    tasks.extend(prover_tasks);

    let particular_crypto_alerts = None;
    let graceful_shutdown = match cfg!(feature = "gpu") {
        true => Some(
            graceful_shutdown(port)
                .await
                .context("failed to prepare graceful shutdown future")?,
        ),
        false => None,
    };
    let tasks_allowed_to_finish = false;
    tokio::select! {
        _ = wait_for_tasks(tasks, particular_crypto_alerts, graceful_shutdown, tasks_allowed_to_finish) => {},
        _ = stop_signal_receiver => {
            tracing::info!("Stop signal received, shutting down");
        },
    }

    stop_sender.send(true).ok();
    Ok(())
}

#[cfg(not(feature = "gpu"))]
async fn get_prover_tasks(
    prover_config: FriProverConfig,
    stop_receiver: Receiver<bool>,
    store_factory: ObjectStoreFactory,
    public_blob_store: Option<Box<dyn ObjectStore>>,
    pool: ConnectionPool,
    circuit_ids_for_round_to_be_proven: Vec<CircuitIdRoundTuple>,
) -> anyhow::Result<Vec<JoinHandle<anyhow::Result<()>>>> {
    use crate::prover_job_processor::{load_setup_data_cache, Prover};
    use zksync_vk_setup_data_server_fri::commitment_utils::get_cached_commitments;

    let vk_commitments = get_cached_commitments();

    tracing::info!(
        "Starting CPU FRI proof generation for with vk_commitments: {:?}",
        vk_commitments
    );

    let setup_load_mode =
        load_setup_data_cache(&prover_config).context("load_setup_data_cache()")?;
    let prover = Prover::new(
        store_factory.create_store().await,
        public_blob_store,
        prover_config,
        pool,
        setup_load_mode,
        circuit_ids_for_round_to_be_proven,
        vk_commitments,
    );
    Ok(vec![tokio::spawn(prover.run(stop_receiver, None))])
}

#[cfg(feature = "gpu")]
async fn get_prover_tasks(
    prover_config: FriProverConfig,
    stop_receiver: Receiver<bool>,
    store_factory: ObjectStoreFactory,
    public_blob_store: Option<Box<dyn ObjectStore>>,
    pool: ConnectionPool,
    circuit_ids_for_round_to_be_proven: Vec<CircuitIdRoundTuple>,
) -> anyhow::Result<Vec<JoinHandle<anyhow::Result<()>>>> {
    use gpu_prover_job_processor::gpu_prover;
    use socket_listener::gpu_socket_listener;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use zksync_prover_fri_types::queue::FixedSizeQueue;

    let setup_load_mode =
        gpu_prover::load_setup_data_cache(&prover_config).context("load_setup_data_cache()")?;
    let witness_vector_queue = FixedSizeQueue::new(prover_config.queue_capacity);
    let shared_witness_vector_queue = Arc::new(Mutex::new(witness_vector_queue));
    let consumer = shared_witness_vector_queue.clone();
    let zone = get_zone().await.context("get_zone()")?;
    let local_ip = local_ip().context("Failed obtaining local IP address")?;
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

    tracing::info!(
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
    Ok(vec![
        tokio::spawn(socket_listener.listen_incoming_connections(stop_receiver.clone())),
        tokio::spawn(prover.run(stop_receiver, None)),
    ])
}
