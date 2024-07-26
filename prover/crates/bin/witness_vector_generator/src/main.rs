#![allow(incomplete_features)] // We have to use generic const exprs.
#![feature(generic_const_exprs)]

use std::time::Duration;

use anyhow::Context as _;
use clap::Parser;
use tokio::sync::{oneshot, watch};
use zksync_core_leftovers::temp_config_store::{load_database_secrets, load_general_config};
use zksync_env_config::object_store::ProverObjectStoreConfig;
use zksync_object_store::ObjectStoreFactory;
use zksync_prover_dal::ConnectionPool;
use zksync_prover_fri_types::PROVER_PROTOCOL_SEMANTIC_VERSION;
use zksync_prover_fri_utils::{get_all_circuit_id_round_tuples_for, region_fetcher::RegionFetcher};
use zksync_queued_job_processor::JobProcessor;
use zksync_utils::wait_for_tasks::ManagedTasks;
use zksync_vlog::prometheus::PrometheusExporterConfig;

use crate::generator::WitnessVectorGenerator;

mod generator;
mod metrics;

#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version)]
struct Cli {
    /// Number of times `witness_vector_generator` should be run.
    #[arg(long)]
    #[arg(short)]
    n_iterations: Option<usize>,
    #[arg(long)]
    pub(crate) config_path: Option<std::path::PathBuf>,
    #[arg(long)]
    pub(crate) secrets_path: Option<std::path::PathBuf>,
    /// Number of WVG jobs to run in parallel.
    /// Default value is 1.
    #[arg(long, default_value_t = 1)]
    pub(crate) threads: usize,
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

    let config = general_config
        .witness_vector_generator
        .context("witness vector generator config")?;
    let specialized_group_id = config.specialized_group_id;
    let exporter_config = PrometheusExporterConfig::pull(config.prometheus_listener_port);

    let pool = ConnectionPool::singleton(database_secrets.prover_url()?)
        .build()
        .await
        .context("failed to build a connection pool")?;
    let object_store_config = ProverObjectStoreConfig(
        general_config
            .prover_config
            .clone()
            .context("prover config")?
            .prover_object_store
            .context("object store")?,
    );
    let object_store = ObjectStoreFactory::new(object_store_config.0)
        .create_store()
        .await?;
    let circuit_ids_for_round_to_be_proven = general_config
        .prover_group_config
        .expect("prover_group_config")
        .get_circuit_ids_for_group_id(specialized_group_id)
        .unwrap_or_default();
    let circuit_ids_for_round_to_be_proven =
        get_all_circuit_id_round_tuples_for(circuit_ids_for_round_to_be_proven);
    let prover_config = general_config.prover_config.context("prover config")?;
    let zone = RegionFetcher::new(
        prover_config.cloud_type,
        prover_config.zone_read_url.clone(),
    )
    .get_zone()
    .await
    .context("get_zone()")?;

    let protocol_version = PROVER_PROTOCOL_SEMANTIC_VERSION;

    let (stop_sender, stop_receiver) = watch::channel(false);

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(stop_signal_sender) = stop_signal_sender.take() {
            stop_signal_sender.send(()).ok();
        }
    })
    .expect("Error setting Ctrl+C handler");

    tracing::info!(
        "Starting {} witness vector generation jobs for group: {} with circuits: {:?} in zone: {} with protocol_version: {:?}",
        opt.threads,
        specialized_group_id,
        circuit_ids_for_round_to_be_proven,
        zone,
        protocol_version
    );

    let mut tasks = vec![tokio::spawn(exporter_config.run(stop_receiver.clone()))];

    for _ in 0..opt.threads {
        let witness_vector_generator = WitnessVectorGenerator::new(
            object_store.clone(),
            pool.clone(),
            circuit_ids_for_round_to_be_proven.clone(),
            zone.clone(),
            config.clone(),
            protocol_version,
            prover_config.max_attempts,
            Some(prover_config.setup_data_path.clone()),
        );
        tasks.push(tokio::spawn(
            witness_vector_generator.run(stop_receiver.clone(), opt.n_iterations),
        ));
    }

    let mut tasks = ManagedTasks::new(tasks);
    tokio::select! {
        _ = tasks.wait_single() => {},
        _ = stop_signal_receiver => {
            tracing::info!("Stop signal received, shutting down");
        }
    };
    stop_sender.send(true).ok();
    tasks.complete(Duration::from_secs(5)).await;
    Ok(())
}
