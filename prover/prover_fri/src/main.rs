#![feature(generic_const_exprs)]

use std::{future::Future, sync::Arc, time::Duration};

use anyhow::Context as _;
use local_ip_address::local_ip;
use prometheus_exporter::PrometheusExporterConfig;
use prover_dal::{ConnectionPool, Prover, ProverDal};
use tokio::{
    sync::{oneshot, watch::Receiver},
    task::JoinHandle,
};
use zksync_config::configs::{
    fri_prover_group::FriProverGroupConfig, FriProverConfig, ObservabilityConfig, PostgresConfig,
};
use zksync_env_config::{
    object_store::{ProverObjectStoreConfig, PublicObjectStoreConfig},
    FromEnv,
};
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_prover_fri_utils::{get_all_circuit_id_round_tuples_for, region_fetcher::get_zone};
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{
    basic_fri_types::CircuitIdRoundTuple,
    prover_dal::{GpuProverInstanceStatus, SocketAddress},
};
use zksync_utils::wait_for_tasks::ManagedTasks;

mod gpu_prover_job_processor;
mod metrics;
mod prover_job_processor;
mod socket_listener;
mod utils;

async fn graceful_shutdown(port: u16) -> anyhow::Result<impl Future<Output = ()>> {
    let postgres_config = PostgresConfig::from_env().context("PostgresConfig::from_env()")?;
    let pool = ConnectionPool::<Prover>::singleton(postgres_config.prover_url()?)
        .build()
        .await
        .context("failed to build a connection pool")?;
    let host = local_ip().context("Failed obtaining local IP address")?;
    let zone_url = &FriProverConfig::from_env()
        .context("FriProverConfig::from_env()")?
        .zone_read_url;
    let zone = get_zone(zone_url).await.context("get_zone()")?;
    let address = SocketAddress { host, port };
    Ok(async move {
        pool.connection()
            .await
            .unwrap()
            .fri_gpu_prover_queue_dal()
            .update_prover_instance_status(address, GpuProverInstanceStatus::Dead, zone)
            .await
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let observability_config =
        ObservabilityConfig::from_env().context("ObservabilityConfig::from_env()")?;
    let log_format: vlog::LogFormat = observability_config
        .log_format
        .parse()
        .context("Invalid log format")?;

    let mut builder = vlog::ObservabilityBuilder::new().with_log_format(log_format);
    if let Some(sentry_url) = &observability_config.sentry_url {
        builder = builder
            .with_sentry_url(sentry_url)
            .expect("Invalid Sentry URL")
            .with_sentry_environment(observability_config.sentry_environment);
    }
    let _guard = builder.build();

    // Report whether sentry is running after the logging subsystem was initialized.
    if let Some(sentry_url) = observability_config.sentry_url {
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
    let object_store_config =
        ProverObjectStoreConfig::from_env().context("ProverObjectStoreConfig::from_env()")?;
    let object_store_factory = ObjectStoreFactory::new(object_store_config.0);
    let public_object_store_config =
        PublicObjectStoreConfig::from_env().context("PublicObjectStoreConfig::from_env()")?;
    let public_blob_store = match prover_config.shall_save_to_public_bucket {
        false => None,
        true => Some(
            ObjectStoreFactory::new(public_object_store_config.0)
                .create_store()
                .await,
        ),
    };
    let specialized_group_id = prover_config.specialized_group_id;

    let circuit_ids_for_round_to_be_proven = FriProverGroupConfig::from_env()
        .context("FriProverGroupConfig::from_env()")?
        .get_circuit_ids_for_group_id(specialized_group_id)
        .unwrap_or_default();
    let circuit_ids_for_round_to_be_proven =
        get_all_circuit_id_round_tuples_for(circuit_ids_for_round_to_be_proven);

    tracing::info!(
        "Starting FRI proof generation for group: {} with circuits: {:?}",
        specialized_group_id,
        circuit_ids_for_round_to_be_proven.clone()
    );
    let postgres_config = PostgresConfig::from_env().context("PostgresConfig::from_env()")?;

    // There are 2 threads using the connection pool:
    // 1. The prover thread, which is used to update the prover job status.
    // 2. The socket listener thread, which is used to update the prover instance status.
    const MAX_POOL_SIZE_FOR_PROVER: u32 = 2;

    let pool = ConnectionPool::builder(postgres_config.prover_url()?, MAX_POOL_SIZE_FOR_PROVER)
        .build()
        .await
        .context("failed to build a connection pool")?;
    let port = prover_config.witness_vector_receiver_port;
    let prover_tasks = get_prover_tasks(
        prover_config,
        stop_receiver.clone(),
        object_store_factory,
        public_blob_store,
        pool,
        circuit_ids_for_round_to_be_proven,
    )
    .await
    .context("get_prover_tasks()")?;

    let mut tasks = vec![tokio::spawn(exporter_config.run(stop_receiver))];
    tasks.extend(prover_tasks);

    let mut tasks = ManagedTasks::new(tasks);
    tokio::select! {
        _ = tasks.wait_single() => {
            if cfg!(feature = "gpu") {
                graceful_shutdown(port)
                    .await
                    .context("failed to prepare graceful shutdown future")?
                    .await;
            }
        },
        _ = stop_signal_receiver => {
            tracing::info!("Stop signal received, shutting down");
        },
    }

    stop_sender.send(true).ok();
    tasks.complete(Duration::from_secs(5)).await;
    Ok(())
}

#[cfg(not(feature = "gpu"))]
async fn get_prover_tasks(
    prover_config: FriProverConfig,
    stop_receiver: Receiver<bool>,
    store_factory: ObjectStoreFactory,
    public_blob_store: Option<Arc<dyn ObjectStore>>,
    pool: ConnectionPool<Prover>,
    circuit_ids_for_round_to_be_proven: Vec<CircuitIdRoundTuple>,
) -> anyhow::Result<Vec<JoinHandle<anyhow::Result<()>>>> {
    use zksync_vk_setup_data_server_fri::commitment_utils::get_cached_commitments;

    use crate::prover_job_processor::{load_setup_data_cache, Prover};

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
    public_blob_store: Option<Arc<dyn ObjectStore>>,
    pool: ConnectionPool<Prover>,
    circuit_ids_for_round_to_be_proven: Vec<CircuitIdRoundTuple>,
) -> anyhow::Result<Vec<JoinHandle<anyhow::Result<()>>>> {
    use gpu_prover_job_processor::gpu_prover;
    use socket_listener::gpu_socket_listener;
    use tokio::sync::Mutex;
    use zksync_prover_fri_types::queue::FixedSizeQueue;

    let setup_load_mode =
        gpu_prover::load_setup_data_cache(&prover_config).context("load_setup_data_cache()")?;
    let witness_vector_queue = FixedSizeQueue::new(prover_config.queue_capacity);
    let shared_witness_vector_queue = Arc::new(Mutex::new(witness_vector_queue));
    let consumer = shared_witness_vector_queue.clone();

    let zone = get_zone(&prover_config.zone_read_url)
        .await
        .context("get_zone()")?;
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
