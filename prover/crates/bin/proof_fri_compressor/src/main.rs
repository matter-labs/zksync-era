#![allow(incomplete_features)] // We have to use generic const exprs.
#![feature(generic_const_exprs)]

use std::env;
use std::time::Duration;

use anyhow::Context as _;
use clap::Parser;
use tokio::sync::{oneshot, watch};
use zksync_core_leftovers::temp_config_store::{load_database_secrets, load_general_config};
use zksync_env_config::object_store::ProverObjectStoreConfig;
use zksync_object_store::ObjectStoreFactory;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_fri_types::PROVER_PROTOCOL_SEMANTIC_VERSION;
use zksync_queued_job_processor::JobProcessor;
use zksync_utils::wait_for_tasks::ManagedTasks;
use zksync_vlog::prometheus::PrometheusExporterConfig;

use crate::{
    compressor::ProofCompressor, initial_setup_keys::download_initial_setup_keys_if_not_present,
};

mod compressor;
mod initial_setup_keys;
mod metrics;

#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version)]
struct Cli {
    /// Number of times proof fri compressor should be run.
    #[arg(long = "n_iterations")]
    #[arg(short)]
    number_of_iterations: Option<usize>,
    #[arg(long)]
    pub(crate) config_path: Option<std::path::PathBuf>,
    #[arg(long)]
    pub(crate) secrets_path: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Cli::parse();

    let general_config = load_general_config(opt.config_path).context("general config")?;
    let database_secrets = load_database_secrets(opt.secrets_path).context("database secrets")?;

    let observability_config = general_config
        .observability
        .expect("observability config")
        .clone();
    let _observability_guard = observability_config.install()?;

    let config = general_config
        .proof_compressor_config
        .context("FriProofCompressorConfig")?;
    let pool = ConnectionPool::<Prover>::singleton(database_secrets.prover_url()?)
        .build()
        .await
        .context("failed to build a connection pool")?;
    let object_store_config = ProverObjectStoreConfig(
        general_config
            .prover_config
            .clone()
            .expect("ProverConfig")
            .prover_object_store
            .context("ProverObjectStoreConfig")?,
    );
    let blob_store = ObjectStoreFactory::new(object_store_config.0)
        .create_store()
        .await?;

    let protocol_version = PROVER_PROTOCOL_SEMANTIC_VERSION;

    let proof_compressor = ProofCompressor::new(
        blob_store,
        pool,
        config.compression_mode,
        config.max_attempts,
        protocol_version,
        general_config
            .prover_config
            .expect("ProverConfig doesn't exist")
            .setup_data_path,
    );

    let (stop_sender, stop_receiver) = watch::channel(false);

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(stop_signal_sender) = stop_signal_sender.take() {
            stop_signal_sender.send(()).ok();
        }
    })
    .expect("Error setting Ctrl+C handler"); // Setting handler should always succeed.

    download_initial_setup_keys_if_not_present(
        &config.universal_setup_path,
        &config.universal_setup_download_url,
    );
    env::set_var("CRS_FILE", config.universal_setup_path.clone());

    tracing::info!("Starting proof compressor");

    let prometheus_config = PrometheusExporterConfig::push(
        config.prometheus_pushgateway_url,
        Duration::from_millis(config.prometheus_push_interval_ms.unwrap_or(100)),
    );
    let tasks = vec![
        tokio::spawn(prometheus_config.run(stop_receiver.clone())),
        tokio::spawn(proof_compressor.run(stop_receiver, opt.number_of_iterations)),
    ];

    let mut tasks = ManagedTasks::new(tasks).allow_tasks_to_finish();
    tokio::select! {
        _ = tasks.wait_single() => {},
        _ = stop_signal_receiver => {
            tracing::info!("Stop signal received, shutting down");
        }
    };
    stop_sender.send_replace(true);
    tasks.complete(Duration::from_secs(5)).await;
    Ok(())
}
