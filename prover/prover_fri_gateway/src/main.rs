use std::time::Duration;

use anyhow::Context as _;
use clap::Parser;
use prometheus_exporter::PrometheusExporterConfig;
use prover_dal::{ConnectionPool, Prover};
use reqwest::Client;
use tokio::sync::{oneshot, watch};
use zksync_config::{
    configs::{
        DatabaseSecrets, FriProverConfig, FriProverGatewayConfig, ObservabilityConfig,
        PostgresConfig,
    },
    ObjectStoreConfig,
};
use zksync_core_leftovers::temp_config_store::{decode_yaml_repr, TempConfigStore};
use zksync_env_config::{object_store::ProverObjectStoreConfig, FromEnv};
use zksync_object_store::ObjectStoreFactory;
use zksync_protobuf_config::proto::config::secrets::Secrets;
use zksync_prover_interface::api::{ProofGenerationDataRequest, SubmitProofRequest};
use zksync_utils::wait_for_tasks::ManagedTasks;

use crate::api_data_fetcher::{PeriodicApiStruct, PROOF_GENERATION_DATA_PATH, SUBMIT_PROOF_PATH};

mod api_data_fetcher;
mod metrics;
mod proof_gen_data_fetcher;
mod proof_submitter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Cli::parse();

    let general_config = match opt.config_path {
        Some(path) => {
            let yaml = std::fs::read_to_string(path).context("Failed to read general config")?;
            decode_yaml_repr::<zksync_protobuf_config::proto::general::GeneralConfig>(&yaml)
                .context("Failed to parse general config")?
        }
        None => load_env_config()?.general(),
    };

    let database_secrets = match opt.secrets_path {
        Some(path) => {
            let yaml = std::fs::read_to_string(path).context("Failed to read secrets")?;
            let secrets = decode_yaml_repr::<Secrets>(&yaml).context("Failed to parse secrets")?;
            secrets
                .database
                .context("failed to parse database secrets")?
        }
        None => DatabaseSecrets::from_env().context("database secrets")?,
    };

    let observability_config = general_config
        .observability
        .context("observability config")?;

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
            .object_store
            .context("object store")?,
    );
    let store_factory = ObjectStoreFactory::new(object_store_config.0);

    let proof_submitter = PeriodicApiStruct {
        blob_store: store_factory.create_store().await?,
        pool: pool.clone(),
        api_url: format!("{}{SUBMIT_PROOF_PATH}", config.api_url),
        poll_duration: config.api_poll_duration(),
        client: Client::new(),
    };
    let proof_gen_data_fetcher = PeriodicApiStruct {
        blob_store: store_factory.create_store().await?,
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

#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version)]
pub(crate) struct Cli {
    #[arg(long)]
    pub(crate) config_path: Option<std::path::PathBuf>,
    #[arg(long)]
    pub(crate) secrets_path: Option<std::path::PathBuf>,
}

fn load_env_config() -> anyhow::Result<TempConfigStore> {
    Ok(TempConfigStore {
        postgres_config: PostgresConfig::from_env().ok(),
        fri_prover_gateway_config: FriProverGatewayConfig::from_env().ok(),
        object_store_config: ObjectStoreConfig::from_env().ok(),
        observability: ObservabilityConfig::from_env().ok(),
        fri_prover_config: FriProverConfig::from_env().ok(),
        ..Default::default()
    })
}
