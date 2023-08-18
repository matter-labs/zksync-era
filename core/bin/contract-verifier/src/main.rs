use std::cell::RefCell;

use zksync_config::{configs::PrometheusConfig, ApiConfig, ContractVerifierConfig};
use zksync_dal::ConnectionPool;
use zksync_queued_job_processor::JobProcessor;
use zksync_utils::wait_for_tasks::wait_for_tasks;

use futures::{channel::mpsc, executor::block_on, SinkExt, StreamExt};
use tokio::sync::watch;

use crate::verifier::ContractVerifier;

pub mod error;
pub mod verifier;
pub mod zksolc_utils;
pub mod zkvyper_utils;

async fn update_compiler_versions(connection_pool: &ConnectionPool) {
    let mut storage = connection_pool.access_storage().await;
    let mut transaction = storage.start_transaction().await;

    let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| ".".into());

    let zksolc_path = format!("{}/etc/zksolc-bin/", zksync_home);
    let zksolc_versions: Vec<String> = std::fs::read_dir(zksolc_path)
        .unwrap()
        .filter_map(|file| {
            let file = file.unwrap();
            let Ok(file_type) = file.file_type() else {
                return None;
            };
            if file_type.is_dir() {
                file.file_name().into_string().ok()
            } else {
                None
            }
        })
        .collect();
    transaction
        .contract_verification_dal()
        .set_zksolc_versions(zksolc_versions)
        .await
        .unwrap();

    let solc_path = format!("{}/etc/solc-bin/", zksync_home);
    let solc_versions: Vec<String> = std::fs::read_dir(solc_path)
        .unwrap()
        .filter_map(|file| {
            let file = file.unwrap();
            let Ok(file_type) = file.file_type() else {
                return None;
            };
            if file_type.is_dir() {
                file.file_name().into_string().ok()
            } else {
                None
            }
        })
        .collect();
    transaction
        .contract_verification_dal()
        .set_solc_versions(solc_versions)
        .await
        .unwrap();

    let zkvyper_path = format!("{}/etc/zkvyper-bin/", zksync_home);
    let zkvyper_versions: Vec<String> = std::fs::read_dir(zkvyper_path)
        .unwrap()
        .filter_map(|file| {
            let file = file.unwrap();
            let Ok(file_type) = file.file_type() else {
                return None;
            };
            if file_type.is_dir() {
                file.file_name().into_string().ok()
            } else {
                None
            }
        })
        .collect();
    transaction
        .contract_verification_dal()
        .set_zkvyper_versions(zkvyper_versions)
        .await
        .unwrap();

    let vyper_path = format!("{}/etc/vyper-bin/", zksync_home);
    let vyper_versions: Vec<String> = std::fs::read_dir(vyper_path)
        .unwrap()
        .filter_map(|file| {
            let file = file.unwrap();
            let Ok(file_type) = file.file_type() else {
                return None;
            };
            if file_type.is_dir() {
                file.file_name().into_string().ok()
            } else {
                None
            }
        })
        .collect();

    transaction
        .contract_verification_dal()
        .set_vyper_versions(vyper_versions)
        .await
        .unwrap();

    transaction.commit().await;
}

use structopt::StructOpt;
use zksync_dal::connection::DbVariant;

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
    let pool = ConnectionPool::singleton(DbVariant::Master).build().await;

    vlog::init();
    let sentry_guard = vlog::init_sentry();
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

    let contract_verifier = ContractVerifier::new(verifier_config, pool);
    let tasks = vec![
        tokio::spawn(contract_verifier.run(stop_receiver, opt.jobs_number)),
        prometheus_exporter::run_prometheus_exporter(prometheus_config.listener_port, None),
    ];

    let particular_crypto_alerts = None;
    let graceful_shutdown = None::<futures::future::Ready<()>>;
    let tasks_allowed_to_finish = false;
    tokio::select! {
        _ = wait_for_tasks(tasks, particular_crypto_alerts, graceful_shutdown, tasks_allowed_to_finish) => {},
        _ = stop_signal_receiver.next() => {
            vlog::info!("Stop signal received, shutting down");
        },
    };
    let _ = stop_sender.send(true);

    // Sleep for some time to let verifier gracefully stop.
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
}
