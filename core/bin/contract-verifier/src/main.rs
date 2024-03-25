use std::{cell::RefCell, time::Duration};

use anyhow::Context as _;
use futures::{channel::mpsc, executor::block_on, SinkExt, StreamExt};
use prometheus_exporter::PrometheusExporterConfig;
use tokio::sync::watch;
use zksync_config::{
    configs::{ObservabilityConfig, PrometheusConfig},
    ApiConfig, ContractVerifierConfig, PostgresConfig,
};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_env_config::FromEnv;
use zksync_queued_job_processor::JobProcessor;
use zksync_utils::wait_for_tasks::ManagedTasks;

use crate::verifier::ContractVerifier;

pub mod error;
pub mod verifier;
pub mod zksolc_utils;
pub mod zkvyper_utils;

async fn update_compiler_versions(connection_pool: &ConnectionPool<Core>) {
    let mut storage = connection_pool.connection().await.unwrap();
    let mut transaction = storage.start_transaction().await.unwrap();

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

    transaction.commit().await.unwrap();
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
async fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();

    let verifier_config = ContractVerifierConfig::from_env().context("ContractVerifierConfig")?;
    let prometheus_config = PrometheusConfig {
        listener_port: verifier_config.prometheus_port,
        ..ApiConfig::from_env().context("ApiConfig")?.prometheus
    };
    let postgres_config = PostgresConfig::from_env().context("PostgresConfig")?;
    let pool = ConnectionPool::<Core>::singleton(
        postgres_config
            .master_url()
            .context("Master DB URL is absent")?,
    )
    .build()
    .await
    .unwrap();

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

    // Report whether sentry is running after the logging subsystem was initialized.
    if let Some(sentry_url) = observability_config.sentry_url {
        tracing::info!("Sentry configured with URL: {sentry_url}");
    } else {
        tracing::info!("No sentry URL was provided");
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
        // TODO PLA-335: Leftovers after the prover DB split.
        // The prover connection pool is not used by the contract verifier, but we need to pass it
        // since `JobProcessor` trait requires it.
        tokio::spawn(contract_verifier.run(stop_receiver.clone(), opt.jobs_number)),
        tokio::spawn(
            PrometheusExporterConfig::pull(prometheus_config.listener_port).run(stop_receiver),
        ),
    ];

    let mut tasks = ManagedTasks::new(tasks);
    tokio::select! {
        () = tasks.wait_single() => {},
        _ = stop_signal_receiver.next() => {
            tracing::info!("Stop signal received, shutting down");
        },
    };
    stop_sender.send_replace(true);

    // Sleep for some time to let verifier gracefully stop.
    tasks.complete(Duration::from_secs(5)).await;
    Ok(())
}
