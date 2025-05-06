use std::time::Duration;

use anyhow::Context as _;
use clap::Parser;
use client::{proof_gen_data_fetcher::ProofGenDataFetcher, proof_submitter::ProofSubmitter};
use proof_data_manager::ProofDataManager;
use tokio::sync::{oneshot, watch};
use traits::PeriodicApi as _;
use zksync_config::configs::fri_prover_gateway::ApiMode;
use zksync_core_leftovers::temp_config_store::{load_database_secrets, load_general_config};
use zksync_env_config::object_store::ProverObjectStoreConfig;
use zksync_object_store::ObjectStoreFactory;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_task_management::ManagedTasks;
use zksync_vlog::prometheus::PrometheusExporterConfig;

mod client;
mod error;
mod metrics;
mod proof_data_manager;
mod server;
mod traits;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Cli::parse();

    let general_config = load_general_config(opt.config_path).context("general config")?;
    let database_secrets = load_database_secrets(opt.secrets_path).context("database secrets")?;

    let observability_config = general_config
        .observability
        .context("observability config")?;
    let _observability_guard = observability_config.install()?;

    let config = general_config
        .prover_gateway
        .context("prover gateway config")?;

    let postgres_config = general_config.postgres_config.context("postgres config")?;
    let pool = ConnectionPool::<Prover>::builder(
        database_secrets.prover_url()?,
        postgres_config.max_connections()?,
    )
    .build()
    .await
    .context("failed to build a connection pool")?;
    let object_store_config = ProverObjectStoreConfig(
        general_config
            .prover_config
            .context("prover config")?
            .prover_object_store
            .context("object store")?,
    );
    let store_factory = ObjectStoreFactory::new(object_store_config.0);

    let (stop_sender, stop_receiver) = watch::channel(false);

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(stop_signal_sender) = stop_signal_sender.take() {
            stop_signal_sender.send(()).ok();
        }
    })
    .context("Error setting Ctrl+C handler")?;

    tracing::info!("Starting Fri Prover Gateway in mode {:?}", config.api_mode);

    let tasks = match &config.api_mode {
        ApiMode::Legacy => {
            let proof_submitter = ProofSubmitter::new(
                store_factory.create_store().await?,
                config.api_url.clone(),
                pool.clone(),
            );
            let proof_gen_data_fetcher = ProofGenDataFetcher::new(
                store_factory.create_store().await?,
                config.api_url.clone(),
                pool,
            );

            vec![
                tokio::spawn(
                    PrometheusExporterConfig::pull(config.prometheus_listener_port)
                        .run(stop_receiver.clone()),
                ),
                tokio::spawn(
                    proof_gen_data_fetcher.run(config.api_poll_duration(), stop_receiver.clone()),
                ),
                tokio::spawn(proof_submitter.run(config.api_poll_duration(), stop_receiver)),
            ]
        }
        ApiMode::ProverCluster => {
            let port = config
                .port
                .expect("Port must be specified in ProverCluster mode");

            let processor = ProofDataManager::new(store_factory.create_store().await?, pool);

            let api = server::Api::new(processor.clone(), port);

            vec![
                tokio::spawn(
                    PrometheusExporterConfig::pull(config.prometheus_listener_port)
                        .run(stop_receiver.clone()),
                ),
                tokio::spawn(api.run(stop_receiver)),
            ]
        }
    };

    let mut tasks = ManagedTasks::new(tasks);
    tokio::select! {
        _ = tasks.wait_single() => {},
         _ = stop_signal_receiver => {
            tracing::info!("Stop signal received, shutting down");
        }
    }

    stop_sender.send(true).ok();
    tasks.complete(Duration::from_secs(5)).await;

    Ok(())
}

#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version)]
pub(crate) struct Cli {
    #[arg(long)]
    pub(crate) config_path: Option<std::path::PathBuf>,
    #[arg(long)]
    pub(crate) secrets_path: Option<std::path::PathBuf>,
}
