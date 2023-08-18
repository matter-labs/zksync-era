#![feature(generic_const_exprs)]

use prometheus_exporter::run_prometheus_exporter;
use structopt::StructOpt;
use tokio::{sync::oneshot, sync::watch};

use crate::generator::WitnessVectorGenerator;
use zksync_config::configs::fri_prover_group::FriProverGroupConfig;
use zksync_config::configs::{AlertsConfig, FriWitnessVectorGeneratorConfig, PrometheusConfig};
use zksync_dal::connection::DbVariant;
use zksync_dal::ConnectionPool;
use zksync_object_store::ObjectStoreFactory;
use zksync_prover_utils::region_fetcher::get_zone;
use zksync_queued_job_processor::JobProcessor;
use zksync_utils::wait_for_tasks::wait_for_tasks;

mod generator;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "zksync_witness_vector_generator",
    about = "Tool for generating witness vectors for circuits"
)]
struct Opt {
    /// Number of times witness_vector_generator should be run.
    #[structopt(short = "n", long = "n_iterations")]
    number_of_iterations: Option<usize>,
}

#[tokio::main]
async fn main() {
    vlog::init();
    let opt = Opt::from_args();
    let config = FriWitnessVectorGeneratorConfig::from_env();
    let prometheus_config = PrometheusConfig {
        listener_port: config.prometheus_listener_port,
        pushgateway_url: config.prometheus_pushgateway_url.clone(),
        push_interval_ms: config.prometheus_push_interval_ms,
    };
    let pool = ConnectionPool::builder(DbVariant::Prover).build().await;
    let blob_store = ObjectStoreFactory::from_env().create_store().await;
    let circuit_ids_for_round_to_be_proven = FriProverGroupConfig::from_env()
        .get_circuit_ids_for_group_id(config.specialized_group_id)
        .unwrap_or(vec![]);
    let zone = get_zone().await;
    let witness_vector_generator = WitnessVectorGenerator::new(
        blob_store,
        pool,
        circuit_ids_for_round_to_be_proven,
        zone,
        config,
    );

    let (stop_sender, stop_receiver) = watch::channel(false);

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(stop_signal_sender) = stop_signal_sender.take() {
            stop_signal_sender.send(()).ok();
        }
    })
    .expect("Error setting Ctrl+C handler");

    vlog::info!("Starting witness vector generation");

    let tasks = vec![
        run_prometheus_exporter(prometheus_config.listener_port, None),
        tokio::spawn(witness_vector_generator.run(stop_receiver, opt.number_of_iterations)),
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
