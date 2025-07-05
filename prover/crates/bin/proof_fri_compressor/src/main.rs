#![allow(incomplete_features)] // We have to use generic const exprs.
#![feature(generic_const_exprs)]

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use clap::Parser;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use zksync_config::{
    configs::{FriProofCompressorConfig, GeneralConfig, PostgresSecrets},
    full_config_schema,
    sources::ConfigFilePaths,
};
use zksync_object_store::ObjectStoreFactory;
use zksync_proof_fri_compressor_service::proof_fri_compressor_runner;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::PROVER_PROTOCOL_SEMANTIC_VERSION;
use zksync_prover_keystore::{compressor::load_all_resources, keystore::Keystore};
use zksync_task_management::ManagedTasks;

use crate::{
    initial_setup_keys::download_initial_setup_keys_if_not_present,
    metrics::PROOF_FRI_COMPRESSOR_INSTANCE_METRICS,
};

mod initial_setup_keys;
mod metrics;

const GRACEFUL_SHUTDOWN_DURATION: Duration = Duration::from_secs(90);

#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version)]
struct Cli {
    /// Number of times proof fri compressor should be run.
    #[arg(long = "n_iterations")]
    #[arg(short)]
    number_of_iterations: Option<usize>,
    #[arg(long)]
    pub(crate) fflonk: Option<bool>,
    #[arg(long)]
    pub(crate) config_path: Option<std::path::PathBuf>,
    #[arg(long)]
    pub(crate) secrets_path: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(sender) = stop_signal_sender.take() {
            sender.send(()).ok();
        }
    })
    .context("Error setting Ctrl+C handler")?;

    let cancellation_token = CancellationToken::new();
    let mut managed_tasks = ManagedTasks::new(vec![]);
    let (metrics_stop_sender, metrics_stop_receiver) = tokio::sync::watch::channel(false);

    tokio::select! {
        _ = run_inner(cancellation_token.clone(), metrics_stop_receiver, &mut managed_tasks) => {},
        _ = stop_signal_receiver => {
            tracing::info!("Stop request received, shutting down");
        }
    }
    let shutdown_time = Instant::now();
    cancellation_token.cancel();
    metrics_stop_sender
        .send(true)
        .context("failed to stop metrics")?;
    managed_tasks.complete(GRACEFUL_SHUTDOWN_DURATION).await;
    tracing::info!("Tasks completed in {:?}.", shutdown_time.elapsed());
    Ok(())
}

async fn run_inner(
    cancellation_token: CancellationToken,
    metrics_stop_receiver: tokio::sync::watch::Receiver<bool>,
    managed_tasks: &mut ManagedTasks,
) -> anyhow::Result<()> {
    let start_time = Instant::now();
    let opt = Cli::parse();
    let is_fflonk = opt.fflonk.unwrap_or(false);
    let schema = full_config_schema();
    let config_file_paths = ConfigFilePaths {
        general: opt.config_path,
        secrets: opt.secrets_path,
        ..ConfigFilePaths::default()
    };
    let config_sources = config_file_paths.into_config_sources("ZKSYNC_")?;

    let _observability_guard = config_sources.observability()?.install()?;

    let mut repo = config_sources.build_repository(&schema);
    let general_config: GeneralConfig = repo.parse()?;
    let database_secrets: PostgresSecrets = repo.parse()?;

    let config = general_config
        .proof_compressor_config
        .context("FriProofCompressorConfig")?;
    let prover_config = general_config
        .prover_config
        .context("ProverConfig doesn't exist")?;
    let object_store_config = prover_config.prover_object_store;

    let prometheus_exporter_config = general_config
        .prometheus_config
        .build_exporter_config(config.prometheus_port)
        .context("Failed to build Prometheus exporter configuration")?;
    tracing::info!("Using Prometheus exporter with {prometheus_exporter_config:?}");

    let mut tasks = vec![tokio::spawn(
        prometheus_exporter_config.run(metrics_stop_receiver),
    )];

    let pool = ConnectionPool::<Prover>::singleton(database_secrets.prover_url()?)
        .build()
        .await
        .context("failed to build a connection pool")?;

    let blob_store = ObjectStoreFactory::new(object_store_config)
        .create_store()
        .await?;
    let protocol_version = PROVER_PROTOCOL_SEMANTIC_VERSION;
    let keystore = Keystore::locate().with_setup_path(Some(prover_config.setup_data_path));

    let l1_verifier_config = pool
        .connection()
        .await?
        .fri_protocol_versions_dal()
        .get_l1_verifier_config()
        .await
        .map_err(|_| anyhow::anyhow!("Failed to get L1 verifier config from database"))?;
    if l1_verifier_config.fflonk_snark_wrapper_vk_hash.is_none() && is_fflonk {
        anyhow::bail!("There was no FFLONK verification hash found in the database while trying to run compressor in FFLONK mode, aborting");
    }

    setup_crs_keys(&config);

    let keystore = Arc::new(keystore);
    load_all_resources(&keystore, is_fflonk);

    PROOF_FRI_COMPRESSOR_INSTANCE_METRICS
        .startup_time
        .set(start_time.elapsed());

    let proof_fri_compressor_runner = proof_fri_compressor_runner(
        pool,
        blob_store,
        protocol_version,
        is_fflonk,
        cancellation_token.clone(),
        keystore,
    );

    tracing::info!("Starting proof compressor");
    tasks.extend(proof_fri_compressor_runner.run());

    *managed_tasks = ManagedTasks::new(tasks);
    managed_tasks.wait_single().await;
    Ok(())
}

fn setup_crs_keys(config: &FriProofCompressorConfig) {
    download_initial_setup_keys_if_not_present(
        &config.universal_setup_path,
        &config.universal_setup_download_url,
    );
    std::env::set_var("COMPACT_CRS_FILE", &config.universal_setup_path);
}
