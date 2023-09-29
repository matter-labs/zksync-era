use prometheus_exporter::run_prometheus_exporter;
use structopt::StructOpt;
use tokio::{sync::oneshot, sync::watch};

use zksync_config::configs::{
    AlertsConfig, CircuitSynthesizerConfig, PrometheusConfig, ProverGroupConfig,
};
use zksync_dal::connection::DbVariant;
use zksync_dal::ConnectionPool;
use zksync_object_store::ObjectStoreFactory;
use zksync_queued_job_processor::JobProcessor;
use zksync_verification_key_server::get_cached_commitments;
use zksync_utils::wait_for_tasks::wait_for_tasks;

use crate::circuit_synthesizer::CircuitSynthesizer;

mod circuit_synthesizer;

#[derive(Debug, StructOpt)]
#[structopt(name = "TODO", about = "TODO")]
struct Opt {
    /// Number of times circuit_synthesizer should be run.
    #[structopt(short = "n", long = "n_iterations")]
    number_of_iterations: Option<usize>,
}

#[tokio::main]
async fn main() {
    vlog::init();
    let opt = Opt::from_args();
    let config: CircuitSynthesizerConfig = CircuitSynthesizerConfig::from_env();
    let pool = ConnectionPool::builder(DbVariant::Prover).build().await;
    let vk_commitments = get_cached_commitments();

    let circuit_synthesizer = CircuitSynthesizer::new(
        config.clone(),
        ProverGroupConfig::from_env(),
        &ObjectStoreFactory::from_env(),
        vk_commitments,
        pool,
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
    let prometheus_config = PrometheusConfig {
        listener_port: config.prometheus_listener_port,
        pushgateway_url: config.prometheus_pushgateway_url,
        push_interval_ms: config.prometheus_push_interval_ms,
    };
    let tasks = vec![
        run_prometheus_exporter(prometheus_config.listener_port, None),
        tokio::spawn(circuit_synthesizer.run(stop_receiver, opt.number_of_iterations)),
    ];

    let particular_crypto_alerts = Some(AlertsConfig::from_env().sporadic_crypto_errors_substrs);
    let graceful_shutdown = None::<futures::future::Ready<()>>;
    let tasks_allowed_to_finish = false;
    tokio::select! {
        _ = wait_for_tasks(tasks, particular_crypto_alerts, graceful_shutdown, tasks_allowed_to_finish) => {},
        _ = stop_signal_receiver => {
            vlog::info!("Stop signal received, shutting down");
        }
    };
    stop_sender.send(true).ok();
}
