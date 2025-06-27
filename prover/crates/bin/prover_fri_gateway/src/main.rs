use std::time::Duration;

use anyhow::Context as _;
use clap::Parser;
use client::{proof_gen_data_fetcher::ProofGenDataFetcher, proof_submitter::ProofSubmitter};
use proof_data_manager::ProofDataManager;
use tokio::sync::{oneshot, watch};
use traits::PeriodicApi as _;
use zksync_object_store::ObjectStoreFactory;
use zksync_prover_config::CompleteProverConfig;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_task_management::ManagedTasks;
use zksync_types::basic_fri_types::ApiMode;

mod client;
mod error;
mod metrics;
mod proof_data_manager;
mod server;
mod traits;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Cli::parse();
    let schema = CompleteProverConfig::schema()?;
    let config_sources = CompleteProverConfig::config_sources(opt.config_path, opt.secrets_path)?;

    let _observability_guard = config_sources.observability()?.install()?;

    let mut repo = config_sources.build_repository(&schema);
    let general_config: CompleteProverConfig = repo.parse()?;

    let config = general_config
        .prover_gateway
        .context("prover gateway config")?;

    let postgres_config = general_config.postgres_config;
    let pool = ConnectionPool::<Prover>::builder(
        postgres_config.prover_url()?,
        postgres_config.max_connections()?,
    )
    .build()
    .await
    .context("failed to build a connection pool")?;
    let object_store_config = general_config
        .prover_config
        .context("prover config")?
        .prover_object_store;
    let store_factory = ObjectStoreFactory::new(object_store_config);

    let (stop_sender, stop_receiver) = watch::channel(false);

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(stop_signal_sender) = stop_signal_sender.take() {
            stop_signal_sender.send(()).ok();
        }
    })
    .context("Error setting Ctrl+C handler")?;

    let prometheus_exporter_config = general_config
        .prometheus_config
        .build_exporter_config(config.prometheus_listener_port)
        .context("Failed to build Prometheus exporter configuration")?;
    tracing::info!("Using Prometheus exporter with {prometheus_exporter_config:?}");

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
                tokio::spawn(prometheus_exporter_config.run(stop_receiver.clone())),
                tokio::spawn(
                    proof_gen_data_fetcher.run(config.api_poll_duration, stop_receiver.clone()),
                ),
                tokio::spawn(proof_submitter.run(config.api_poll_duration, stop_receiver)),
            ]
        }
        ApiMode::ProverCluster => {
            let port = config
                .port
                .expect("Port must be specified in ProverCluster mode");

            let processor = ProofDataManager::new(store_factory.create_store().await?, pool);

            let api = server::Api::new(processor.clone(), port);

            vec![
                tokio::spawn(prometheus_exporter_config.run(stop_receiver.clone())),
                tokio::spawn(api.run(stop_receiver)),
            ]
        }
    };

    let mut tasks = ManagedTasks::new(tasks);
    tokio::select! {
        _ = tasks.wait_single() => {},
         _ = stop_signal_receiver => {
            tracing::info!("Stop request received, shutting down");
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
