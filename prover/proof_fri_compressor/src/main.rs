use structopt::StructOpt;
use tokio::{sync::oneshot, sync::watch};

use prometheus_exporter::run_prometheus_exporter;
use zksync_config::configs::{FriProofCompressorConfig, PrometheusConfig};
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
    vlog::init();
    let opt = Opt::from_args();
    let config = FriProofCompressorConfig::from_env();
    let prometheus_config = PrometheusConfig {
        listener_port: config.prometheus_listener_port,
        pushgateway_url: config.prometheus_pushgateway_url.clone(),
        push_interval_ms: config.prometheus_push_interval_ms,
    };
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

    vlog::info!("Starting proof compressor");

    let tasks = vec![
        run_prometheus_exporter(prometheus_config.listener_port, None),
        tokio::spawn(proof_compressor.run(stop_receiver, opt.number_of_iterations)),
    ];

    let graceful_shutdown = None::<futures::future::Ready<()>>;
    let tasks_allowed_to_finish = false;
    tokio::select! {
        _ = wait_for_tasks(tasks, None, graceful_shutdown, tasks_allowed_to_finish) => {},
        _ = stop_signal_receiver => {
            vlog::info!("Stop signal received, shutting down");
        }
    };
    stop_sender.send(true).ok();
}
