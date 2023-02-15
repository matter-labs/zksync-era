use std::cell::RefCell;

use zksync_config::{
    configs::utils::Prometheus as PrometheusConfig, ApiConfig, ContractVerifierConfig,
};
use zksync_dal::ConnectionPool;
use zksync_queued_job_processor::JobProcessor;

use futures::{channel::mpsc, executor::block_on, future, SinkExt, StreamExt};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::verifier::ContractVerifier;

pub mod error;
pub mod verifier;
pub mod zksolc_utils;

pub async fn wait_for_tasks(task_futures: Vec<JoinHandle<()>>) {
    match future::select_all(task_futures).await.0 {
        Ok(_) => {
            vlog::info!("One of the actors finished its run, while it wasn't expected to do it");
        }
        Err(error) => {
            vlog::info!(
                "One of the tokio actors unexpectedly finished with error: {:?}",
                error
            );
        }
    }
}

async fn update_compiler_versions(connection_pool: &ConnectionPool) {
    let mut storage = connection_pool.access_storage().await;
    let mut transaction = storage.start_transaction().await;

    let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| ".".into());

    let zksolc_path = format!("{}/etc/zksolc-bin/", zksync_home);
    let zksolc_versions: Vec<String> = std::fs::read_dir(zksolc_path)
        .unwrap()
        .filter_map(|file| {
            let file = file.unwrap();
            if file.file_type().unwrap().is_dir() {
                Some(file.file_name().into_string().unwrap())
            } else {
                None
            }
        })
        .collect();
    transaction
        .explorer()
        .contract_verification_dal()
        .set_zksolc_versions(zksolc_versions)
        .unwrap();

    let solc_path = format!("{}/etc/solc-bin/", zksync_home);
    let solc_versions: Vec<String> = std::fs::read_dir(solc_path)
        .unwrap()
        .filter_map(|file| {
            let file = file.unwrap();
            if file.file_type().unwrap().is_dir() {
                Some(file.file_name().into_string().unwrap())
            } else {
                None
            }
        })
        .collect();
    transaction
        .explorer()
        .contract_verification_dal()
        .set_solc_versions(solc_versions)
        .unwrap();

    transaction.commit().await;
}

use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(name = "zkSync contract code verifier", author = "Matter Labs")]
struct Opt {
    /// Number of jobs to process. If None, runs indefinitely.
    #[structopt(long)]
    jobs_number: Option<usize>,
}

#[tokio::main]
async fn main() {
    let opt = Opt::from_args();

    let verifier_config = ContractVerifierConfig::from_env();
    let prometheus_config = PrometheusConfig {
        listener_port: verifier_config.prometheus_port,
        ..ApiConfig::from_env().prometheus
    };
    let pool = ConnectionPool::new(Some(1), true);

    let sentry_guard = vlog::init();
    match sentry_guard {
        Some(_) => vlog::info!(
            "Starting Sentry url: {}",
            std::env::var("MISC_SENTRY_URL").unwrap(),
        ),
        None => vlog::info!("No sentry url configured"),
    }

    let (stop_sender, stop_receiver) = watch::channel(false);
    let (stop_signal_sender, mut stop_signal_receiver) = mpsc::channel(256);
    {
        let stop_signal_sender = RefCell::new(stop_signal_sender.clone());
        ctrlc::set_handler(move || {
            let mut sender = stop_signal_sender.borrow_mut();
            block_on(sender.send(true)).expect("Ctrl+C signal send");
        })
        .expect("Error setting Ctrl+C handler");
    }

    update_compiler_versions(&pool).await;

    let contract_verifier = ContractVerifier::new(verifier_config);
    let tasks = vec![
        tokio::spawn(contract_verifier.run(pool, stop_receiver, opt.jobs_number)),
        prometheus_exporter::run_prometheus_exporter(prometheus_config, false),
    ];
    tokio::select! {
        _ = async { wait_for_tasks(tasks).await } => {},
        _ = async { stop_signal_receiver.next().await } => {
            vlog::info!("Stop signal received, shutting down");
        },
    };
    let _ = stop_sender.send(true);

    // Sleep for some time to let verifier gracefully stop.
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
}
