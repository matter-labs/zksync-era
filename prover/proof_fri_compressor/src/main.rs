use std::{env, time::Duration};

use anyhow::Context as _;
use prometheus_exporter::PrometheusExporterConfig;
use prover_dal::{ConnectionPool, Prover};
use structopt::StructOpt;
use tokio::sync::{oneshot, watch};
use zksync_config::configs::{FriProofCompressorConfig, ObservabilityConfig, PostgresConfig};
use zksync_env_config::{object_store::ProverObjectStoreConfig, FromEnv};
use zksync_object_store::ObjectStoreFactory;
use zksync_queued_job_processor::JobProcessor;
use zksync_utils::wait_for_tasks::ManagedTasks;

use crate::{
    compressor::ProofCompressor, initial_setup_keys::download_initial_setup_keys_if_not_present,
};

mod compressor;
mod initial_setup_keys;
mod metrics;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "zksync_proof_fri_compressor",
    about = "Tool for compressing FRI proofs to old bellman proof"
)]
struct Opt {
    /// Number of times proof fri compressor should be run.
    #[structopt(short = "n", long = "n_iterations")]
    number_of_iterations: Option<usize>,
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

    let opt = Opt::from_args();
    let config = FriProofCompressorConfig::from_env().context("FriProofCompressorConfig")?;
    let postgres_config = PostgresConfig::from_env().context("PostgresConfig::from_env()")?;
    let pool = ConnectionPool::<Prover>::singleton(postgres_config.prover_url()?)
        .build()
        .await
        .context("failed to build a connection pool")?;
    let object_store_config =
        ProverObjectStoreConfig::from_env().context("ProverObjectStoreConfig::from_env()")?;
    let blob_store = ObjectStoreFactory::new(object_store_config.0)
        .create_store()
        .await;
    let proof_compressor = ProofCompressor::new(
        blob_store,
        pool,
        config.compression_mode,
        config.verify_wrapper_proof,
        config.max_attempts,
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
