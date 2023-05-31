use futures::future;
use structopt::StructOpt;
use tokio::{sync::oneshot, sync::watch, task::JoinHandle};

use prometheus_exporter::run_prometheus_exporter;
use zksync_config::configs::{utils::Prometheus, CircuitSynthesizerConfig, ProverGroupConfig};
use zksync_dal::ConnectionPool;
use zksync_object_store::ObjectStoreFactory;
use zksync_queued_job_processor::JobProcessor;

use crate::circuit_synthesizer::CircuitSynthesizer;

mod circuit_synthesizer;

#[derive(Debug, StructOpt)]
#[structopt(name = "TODO", about = "TODO")]
struct Opt {
    /// Number of times circuit_synthesizer should be run.
    #[structopt(short = "n", long = "n_iterations")]
    number_of_iterations: Option<usize>,
}

async fn wait_for_tasks(task_futures: Vec<JoinHandle<()>>) {
    match future::select_all(task_futures).await.0 {
        Ok(_) => {
            vlog::info!("One of the actors finished its run, while it wasn't expected to do it");
        }
        Err(err) => {
            vlog::info!("One of the tokio actors unexpectedly finished with error: {err:?}");
        }
    }
}

#[tokio::main]
async fn main() {
    let opt = Opt::from_args();
    let sentry_guard = vlog::init();
    match sentry_guard {
        Some(_) => vlog::info!(
            "Starting Sentry url: {}",
            std::env::var("MISC_SENTRY_URL").unwrap(),
        ),
        None => vlog::info!("No sentry url configured"),
    }
    let config: CircuitSynthesizerConfig = CircuitSynthesizerConfig::from_env();
    let pool = ConnectionPool::new(None, true);

    let circuit_synthesizer = CircuitSynthesizer::new(
        config.clone(),
        ProverGroupConfig::from_env(),
        &ObjectStoreFactory::from_env(),
    )
    .await
    .unwrap_or_else(|err| {
        vlog::error!("Could not initialize synthesizer: {err:?}");
        panic!("Could not initialize synthesizer: {err:?}");
    });

    let (stop_sender, stop_receiver) = watch::channel(false);

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(stop_signal_sender) = stop_signal_sender.take() {
            stop_signal_sender.send(()).ok();
        }
    })
    .expect("Error setting Ctrl+C handler");

    vlog::info!("Starting circuit synthesizer");
    let prometheus_config = Prometheus {
        listener_port: config.prometheus_listener_port,
        pushgateway_url: config.prometheus_pushgateway_url,
        push_interval_ms: config.prometheus_push_interval_ms,
    };
    let tasks = vec![
        run_prometheus_exporter(prometheus_config, true),
        tokio::spawn(circuit_synthesizer.run(pool, stop_receiver, opt.number_of_iterations)),
    ];

    tokio::select! {
        _ = wait_for_tasks(tasks) => {},
        _ = stop_signal_receiver => {
            vlog::info!("Stop signal received, shutting down");
        }
    };
    stop_sender.send(true).ok();
}
