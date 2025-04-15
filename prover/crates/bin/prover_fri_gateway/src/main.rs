use std::time::Duration;

use anyhow::Context as _;
use clap::Parser;
use proof_gen_data_fetcher::ProofGenDataFetcher;
use proof_submitter::ProofSubmitter;
use tokio::sync::{oneshot, watch};
use traits::PeriodicApi as _;
use zksync_config::{
    configs::{DatabaseSecrets, GeneralConfig, ObservabilityConfig},
    full_config_schema,
    sources::ConfigFilePaths,
    ConfigRepository, ParseResultExt,
};
use zksync_object_store::ObjectStoreFactory;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_task_management::ManagedTasks;
use zksync_vlog::prometheus::PrometheusExporterConfig;

mod client;
mod metrics;
mod proof_gen_data_fetcher;
mod proof_submitter;
mod traits;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Cli::parse();
    let schema = full_config_schema(false);
    let config_file_paths = ConfigFilePaths {
        general: opt.config_path,
        secrets: opt.secrets_path,
        ..ConfigFilePaths::default()
    };
    let config_sources = config_file_paths.into_config_sources("")?;

    let observability_config =
        ObservabilityConfig::from_sources(config_sources.clone()).context("ObservabilityConfig")?;
    let _observability_guard = observability_config.install()?;

    let mut repo = ConfigRepository::new(&schema).with_all(config_sources);
    repo.deserializer_options().coerce_variant_names = true;
    let general_config: GeneralConfig = repo.single()?.parse().log_all_errors()?;
    let database_secrets: DatabaseSecrets = repo.single()?.parse().log_all_errors()?;

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
    let object_store_config = general_config
        .prover_config
        .context("prover config")?
        .prover_object_store;
    let store_factory = ObjectStoreFactory::new(object_store_config);

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

    let (stop_sender, stop_receiver) = watch::channel(false);

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(stop_signal_sender) = stop_signal_sender.take() {
            stop_signal_sender.send(()).ok();
        }
    })
    .context("Error setting Ctrl+C handler")?;

    tracing::info!("Starting Fri Prover Gateway");

    let tasks = vec![
        tokio::spawn(
            PrometheusExporterConfig::pull(config.prometheus_listener_port)
                .run(stop_receiver.clone()),
        ),
        tokio::spawn(proof_gen_data_fetcher.run(config.api_poll_duration(), stop_receiver.clone())),
        tokio::spawn(proof_submitter.run(config.api_poll_duration(), stop_receiver)),
    ];

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
