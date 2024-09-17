#![allow(incomplete_features)] // We have to use generic const exprs.
#![feature(generic_const_exprs)]

use std::{future::Future, sync::Arc, time::Duration};

use anyhow::Context as _;
use clap::Parser;
use local_ip_address::local_ip;
use tokio::{
    sync::{oneshot, watch::Receiver, Notify},
    task::JoinHandle,
};
use zksync_config::configs::{DatabaseSecrets, FriProverConfig};
use zksync_core_leftovers::temp_config_store::{load_database_secrets, load_general_config};
use zksync_env_config::FromEnv;
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::PROVER_PROTOCOL_SEMANTIC_VERSION;
use zksync_prover_fri_utils::{
    get_all_circuit_id_round_tuples_for,
    region_fetcher::{RegionFetcher, Zone},
};
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{
    basic_fri_types::CircuitIdRoundTuple,
    prover_dal::{GpuProverInstanceStatus, SocketAddress},
};
use zksync_utils::wait_for_tasks::ManagedTasks;
use zksync_vlog::prometheus::PrometheusExporterConfig;

mod gpu_prover_availability_checker;
mod gpu_prover_job_processor;
mod metrics;
mod prover_job_processor;
mod socket_listener;
mod utils;

async fn graceful_shutdown(zone: Zone, port: u16) -> anyhow::Result<impl Future<Output = ()>> {
    let database_secrets = DatabaseSecrets::from_env().context("DatabaseSecrets::from_env()")?;
    let pool = ConnectionPool::<Prover>::singleton(database_secrets.prover_url()?)
        .build()
        .await
        .context("failed to build a connection pool")?;
    let host = local_ip().context("Failed obtaining local IP address")?;
    let address = SocketAddress { host, port };
    Ok(async move {
        pool.connection()
            .await
            .unwrap()
            .fri_gpu_prover_queue_dal()
            .update_prover_instance_status(address, GpuProverInstanceStatus::Dead, zone.to_string())
            .await
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Cli::parse();

    let general_config = load_general_config(opt.config_path).context("general config")?;
    let database_secrets = load_database_secrets(opt.secrets_path).context("database secrets")?;

    let observability_config = general_config
        .observability
        .context("observability config")?;
    let _observability_guard = observability_config.install()?;

    let prover_config = general_config.prover_config.context("fri_prover config")?;
    let exporter_config = PrometheusExporterConfig::pull(prover_config.prometheus_port);

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(sender) = stop_signal_sender.take() {
            sender.send(()).ok();
        }
    })
    .context("Error setting Ctrl+C handler")?;

    let zone = RegionFetcher::new(
        prover_config.cloud_type,
        prover_config.zone_read_url.clone(),
    )
    .get_zone()
    .await?;

    let (stop_sender, stop_receiver) = tokio::sync::watch::channel(false);
    let prover_object_store_config = prover_config
        .prover_object_store
        .clone()
        .context("prover object store config")?;
    let object_store_factory = ObjectStoreFactory::new(prover_object_store_config);
    let public_object_store_config = prover_config
        .public_object_store
        .clone()
        .context("public object store config")?;
    let public_blob_store = match prover_config.shall_save_to_public_bucket {
        false => None,
        true => Some(
            ObjectStoreFactory::new(public_object_store_config)
                .create_store()
                .await?,
        ),
    };
    let specialized_group_id = prover_config.specialized_group_id;

    let circuit_ids_for_round_to_be_proven = general_config
        .prover_group_config
        .context("prover group config")?
        .get_circuit_ids_for_group_id(specialized_group_id)
        .unwrap_or_default();
    let circuit_ids_for_round_to_be_proven =
        get_all_circuit_id_round_tuples_for(circuit_ids_for_round_to_be_proven);

    tracing::info!(
        "Starting FRI proof generation for group: {} with circuits: {:?}",
        specialized_group_id,
        circuit_ids_for_round_to_be_proven.clone()
    );

    // There are 2 threads using the connection pool:
    // 1. The prover thread, which is used to update the prover job status.
    // 2. The socket listener thread, which is used to update the prover instance status.
    const MAX_POOL_SIZE_FOR_PROVER: u32 = 2;

    let pool = ConnectionPool::builder(database_secrets.prover_url()?, MAX_POOL_SIZE_FOR_PROVER)
        .build()
        .await
        .context("failed to build a connection pool")?;
    let port = prover_config.witness_vector_receiver_port;

    let notify = Arc::new(Notify::new());

    let prover_tasks = get_prover_tasks(
        prover_config,
        zone.clone(),
        stop_receiver.clone(),
        object_store_factory,
        public_blob_store,
        pool,
        circuit_ids_for_round_to_be_proven,
        opt.max_allocation,
        notify,
    )
    .await
    .context("get_prover_tasks()")?;

    let mut tasks = vec![tokio::spawn(exporter_config.run(stop_receiver))];

    tasks.extend(prover_tasks);

    let mut tasks = ManagedTasks::new(tasks);
    tokio::select! {
        _ = tasks.wait_single() => {
            if cfg!(feature = "gpu") {
                graceful_shutdown(zone, port)
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

#[allow(clippy::too_many_arguments)]
#[cfg(not(feature = "gpu"))]
async fn get_prover_tasks(
    prover_config: FriProverConfig,
    _zone: Zone,
    stop_receiver: Receiver<bool>,
    store_factory: ObjectStoreFactory,
    public_blob_store: Option<Arc<dyn ObjectStore>>,
    pool: ConnectionPool<Prover>,
    circuit_ids_for_round_to_be_proven: Vec<CircuitIdRoundTuple>,
    _max_allocation: Option<usize>,
    _init_notifier: Arc<Notify>,
) -> anyhow::Result<Vec<JoinHandle<anyhow::Result<()>>>> {
    use zksync_prover_keystore::keystore::Keystore;

    use crate::prover_job_processor::{load_setup_data_cache, Prover};

    let protocol_version = PROVER_PROTOCOL_SEMANTIC_VERSION;

    tracing::info!(
        "Starting CPU FRI proof generation for with protocol_version: {:?}",
        protocol_version
    );

    let keystore =
        Keystore::locate().with_setup_path(Some(prover_config.setup_data_path.clone().into()));
    let setup_load_mode =
        load_setup_data_cache(&keystore, &prover_config).context("load_setup_data_cache()")?;
    let prover = Prover::new(
        store_factory.create_store().await?,
        public_blob_store,
        prover_config,
        keystore,
        pool,
        setup_load_mode,
        circuit_ids_for_round_to_be_proven,
        protocol_version,
    );
    Ok(vec![tokio::spawn(prover.run(stop_receiver, None))])
}

#[allow(clippy::too_many_arguments)]
#[cfg(feature = "gpu")]
async fn get_prover_tasks(
    prover_config: FriProverConfig,
    zone: Zone,
    stop_receiver: Receiver<bool>,
    store_factory: ObjectStoreFactory,
    public_blob_store: Option<Arc<dyn ObjectStore>>,
    pool: ConnectionPool<Prover>,
    circuit_ids_for_round_to_be_proven: Vec<CircuitIdRoundTuple>,
    max_allocation: Option<usize>,
    init_notifier: Arc<Notify>,
) -> anyhow::Result<Vec<JoinHandle<anyhow::Result<()>>>> {
    use gpu_prover_job_processor::gpu_prover;
    use socket_listener::gpu_socket_listener;
    use tokio::sync::Mutex;
    use zksync_prover_fri_types::queue::FixedSizeQueue;
    use zksync_prover_keystore::keystore::Keystore;

    let keystore =
        Keystore::locate().with_setup_path(Some(prover_config.setup_data_path.clone().into()));
    let setup_load_mode = gpu_prover::load_setup_data_cache(
        &keystore,
        prover_config.setup_load_mode,
        prover_config.specialized_group_id,
        &circuit_ids_for_round_to_be_proven,
    )
    .await
    .context("load_setup_data_cache()")?;
    let witness_vector_queue = FixedSizeQueue::new(prover_config.queue_capacity);
    let shared_witness_vector_queue = Arc::new(Mutex::new(witness_vector_queue));
    let consumer = shared_witness_vector_queue.clone();

    let local_ip = local_ip().context("Failed obtaining local IP address")?;
    let address = SocketAddress {
        host: local_ip,
        port: prover_config.witness_vector_receiver_port,
    };

    let protocol_version = PROVER_PROTOCOL_SEMANTIC_VERSION;

    let prover = gpu_prover::Prover::new(
        keystore,
        store_factory.create_store().await?,
        public_blob_store,
        prover_config.clone(),
        pool.clone(),
        setup_load_mode,
        circuit_ids_for_round_to_be_proven.clone(),
        consumer,
        address.clone(),
        zone.clone(),
        protocol_version,
        max_allocation,
    );
    let producer = shared_witness_vector_queue.clone();

    tracing::info!(
        "local IP address is: {:?} in zone: {}",
        local_ip,
        zone.clone()
    );
    let socket_listener = gpu_socket_listener::SocketListener::new(
        address.clone(),
        producer,
        pool.clone(),
        prover_config.specialized_group_id,
        zone.clone(),
        protocol_version,
    );

    let mut tasks = vec![
        tokio::spawn(
            socket_listener
                .listen_incoming_connections(stop_receiver.clone(), init_notifier.clone()),
        ),
        tokio::spawn(prover.run(stop_receiver.clone(), None)),
    ];

    // TODO(PLA-874): remove the check after making the availability checker required
    if let Some(check_interval) = prover_config.availability_check_interval_in_secs {
        let availability_checker =
            gpu_prover_availability_checker::availability_checker::AvailabilityChecker::new(
                address,
                zone,
                check_interval,
                pool,
            );

        tasks.push(tokio::spawn(
            availability_checker.run(stop_receiver.clone(), init_notifier),
        ));
    }

    Ok(tasks)
}

#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version)]
pub(crate) struct Cli {
    #[arg(long)]
    pub(crate) config_path: Option<std::path::PathBuf>,
    #[arg(long)]
    pub(crate) secrets_path: Option<std::path::PathBuf>,
    #[arg(long)]
    pub(crate) max_allocation: Option<usize>,
}
