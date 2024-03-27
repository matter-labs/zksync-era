use std::time::Duration;

use anyhow::Context as _;
use prometheus_exporter::PrometheusExporterConfig;
use prover_dal::{ConnectionPool, Prover};
use reqwest::Client;
use tokio::sync::{oneshot, watch};
use zksync_config::configs::{FriProverGatewayConfig, ObservabilityConfig, PostgresConfig};
use zksync_env_config::{object_store::ProverObjectStoreConfig, FromEnv};
use zksync_object_store::ObjectStoreFactory;
use zksync_prover_interface::api::{ProofGenerationDataRequest, SubmitProofRequest};
use zksync_utils::wait_for_tasks::ManagedTasks;

use crate::api_data_fetcher::{PeriodicApiStruct, PROOF_GENERATION_DATA_PATH, SUBMIT_PROOF_PATH};

mod api_data_fetcher;
mod metrics;
mod proof_gen_data_fetcher;
mod proof_submitter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

    let config =
        FriProverGatewayConfig::from_env().context("FriProverGatewayConfig::from_env()")?;
    let postgres_config = PostgresConfig::from_env().context("PostgresConfig::from_env()")?;
    let pool = ConnectionPool::<Prover>::builder(
        postgres_config.prover_url()?,
        postgres_config.max_connections()?,
    )
    .build()
    .await
    .context("failed to build a connection pool")?;
    let object_store_config =
        ProverObjectStoreConfig::from_env().context("ProverObjectStoreConfig::from_env()")?;
    let store_factory = ObjectStoreFactory::new(object_store_config.0);

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
    .context("Error setting Ctrl+C handler")?;

    tracing::info!("Starting Fri Prover Gateway");

    let tasks = vec![
        tokio::spawn(
            PrometheusExporterConfig::pull(config.prometheus_listener_port)
                .run(stop_receiver.clone()),
        ),
        tokio::spawn(
            proof_gen_data_fetcher.run::<ProofGenerationDataRequest>(stop_receiver.clone()),
        ),
        tokio::spawn(proof_submitter.run::<SubmitProofRequest>(stop_receiver)),
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
