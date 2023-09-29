use reqwest::Client;
use tokio::{sync::oneshot, sync::watch};

use crate::api_data_fetcher::{PeriodicApiStruct, PROOF_GENERATION_DATA_PATH, SUBMIT_PROOF_PATH};
use prometheus_exporter::run_prometheus_exporter;
use zksync_config::configs::{FriProverGatewayConfig, PrometheusConfig};
use zksync_dal::connection::DbVariant;
use zksync_dal::ConnectionPool;
use zksync_object_store::ObjectStoreFactory;
use zksync_types::prover_server_api::{ProofGenerationDataRequest, SubmitProofRequest};
use zksync_utils::wait_for_tasks::wait_for_tasks;

mod api_data_fetcher;
mod proof_gen_data_fetcher;
mod proof_submitter;

#[tokio::main]
async fn main() {
    vlog::init();
    let config = FriProverGatewayConfig::from_env();
    let prometheus_config = PrometheusConfig {
        listener_port: config.prometheus_listener_port,
        pushgateway_url: config.prometheus_pushgateway_url.clone(),
        push_interval_ms: config.prometheus_push_interval_ms,
    };
    let pool = ConnectionPool::builder(DbVariant::Prover).build().await;
    let store_factory = ObjectStoreFactory::prover_from_env();

    let proof_submitter = PeriodicApiStruct {
        blob_store: store_factory.create_store().await,
        pool: pool.clone(),
        api_url: format!("{}{SUBMIT_PROOF_PATH}", config.api_url),
        poll_duration: config.api_poll_duration(),
        client: Client::new(),
    };
    let proof_gen_data_fetcher = PeriodicApiStruct {
        blob_store: store_factory.create_store().await,
        pool,
        api_url: format!("{}{PROOF_GENERATION_DATA_PATH}", config.api_url),
        poll_duration: config.api_poll_duration(),
        client: Client::new(),
    };

    let (stop_sender, stop_receiver) = watch::channel(false);

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(stop_signal_sender) = stop_signal_sender.take() {
            stop_signal_sender.send(()).ok();
        }
    })
    .expect("Error setting Ctrl+C handler");

    vlog::info!("Starting Fri Prover Gateway");

    let tasks = vec![
        run_prometheus_exporter(prometheus_config.listener_port, None),
        tokio::spawn(
            proof_gen_data_fetcher.run::<ProofGenerationDataRequest>(stop_receiver.clone()),
        ),
        tokio::spawn(proof_submitter.run::<SubmitProofRequest>(stop_receiver)),
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
