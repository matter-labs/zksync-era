use structopt::StructOpt;
use tokio::{sync::oneshot, sync::watch};

use std::time::Duration;

use prometheus_exporter::PrometheusExporterConfig;
use zksync_config::configs::FriProofCompressorConfig;
use zksync_dal::connection::DbVariant;
use zksync_dal::ConnectionPool;
use zksync_object_store::ObjectStoreFactory;
use zksync_queued_job_processor::JobProcessor;
use zksync_utils::wait_for_tasks::wait_for_tasks;

use crate::compressor::ProofCompressor;

mod compressor;

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
async fn main() {
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let log_format = vlog::log_format_from_env();
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let sentry_url = vlog::sentry_url_from_env();
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let environment = vlog::environment_from_env();

    let mut builder = vlog::ObservabilityBuilder::new().with_log_format(log_format);
    if let Some(sentry_url) = sentry_url {
        builder = builder
            .with_sentry_url(&sentry_url)
            .expect("Invalid Sentry URL")
            .with_sentry_environment(environment);
    }
    let _guard = builder.build();
    
    let opt = Opt::from_args();
    let config = FriProofCompressorConfig::from_env();
    let pool = ConnectionPool::builder(DbVariant::Prover).build().await;
    let blob_store = ObjectStoreFactory::prover_from_env().create_store().await;
    let proof_compressor = ProofCompressor::new(blob_store, pool, config.compression_mode);

    let (stop_sender, stop_receiver) = watch::channel(false);

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(stop_signal_sender) = stop_signal_sender.take() {
            stop_signal_sender.send(()).ok();
        }
    })
    .expect("Error setting Ctrl+C handler");

    tracing::info!("Starting proof compressor");

    let prometheus_config = PrometheusExporterConfig::push(
        config.prometheus_pushgateway_url,
        Duration::from_millis(config.prometheus_push_interval_ms.unwrap_or(100)),
    );
    let tasks = vec![
        tokio::spawn(prometheus_config.run(stop_receiver.clone())),
        tokio::spawn(proof_compressor.run(stop_receiver, opt.number_of_iterations)),
    ];

    let graceful_shutdown = None::<futures::future::Ready<()>>;
    let tasks_allowed_to_finish = false;
    tokio::select! {
        _ = wait_for_tasks(tasks, None, graceful_shutdown, tasks_allowed_to_finish) => {},
        _ = stop_signal_receiver => {
            tracing::info!("Stop signal received, shutting down");
        }
    };
    stop_sender.send(true).ok();
}
