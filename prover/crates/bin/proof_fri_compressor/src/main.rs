#![allow(incomplete_features)] // We have to use generic const exprs.
#![feature(generic_const_exprs)]

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use clap::Parser;
use proof_compression_gpu::CompressorBlobStorage;
// use proof_compression_gpu::SnarkWrapperSetup;
use tokio_util::sync::CancellationToken;
use zksync_config::configs::FriProofCompressorConfig;
use zksync_core_leftovers::temp_config_store::{load_database_secrets, load_general_config};
use zksync_env_config::object_store::ProverObjectStoreConfig;
use zksync_object_store::ObjectStoreFactory;
use zksync_proof_fri_compressor_service::proof_fri_compressor_runner;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_fri_types::PROVER_PROTOCOL_SEMANTIC_VERSION;
use zksync_prover_keystore::keystore::Keystore;
use zksync_task_management::ManagedTasks;
use zksync_vlog::prometheus::PrometheusExporterConfig;

use crate::{
    initial_setup_keys::download_initial_setup_keys_if_not_present,
    metrics::PROOF_FRI_COMPRESSOR_INSTANCE_METRICS,
};

mod initial_setup_keys;
mod metrics;

const GRACEFUL_SHUTDOWN_DURATION: Duration = Duration::from_secs(180);

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
    let start_time = Instant::now();
    let opt = Cli::parse();

    let is_fflonk = opt.fflonk.unwrap_or(false);

    let general_config = load_general_config(opt.config_path).context("general config")?;
    let database_secrets = load_database_secrets(opt.secrets_path).context("database secrets")?;

    let observability_config = general_config
        .observability
        .expect("observability config")
        .clone();
    let _observability_guard = observability_config.install()?;

    let config = general_config
        .proof_compressor_config
        .context("FriProofCompressorConfig")?;
    let pool = ConnectionPool::<Prover>::singleton(database_secrets.prover_url()?)
        .build()
        .await
        .context("failed to build a connection pool")?;
    let object_store_config = ProverObjectStoreConfig(
        general_config
            .prover_config
            .clone()
            .expect("ProverConfig")
            .prover_object_store
            .context("ProverObjectStoreConfig")?,
    );
    let blob_store = ObjectStoreFactory::new(object_store_config.0)
        .create_store()
        .await?;

    let protocol_version = PROVER_PROTOCOL_SEMANTIC_VERSION;

    let prover_config = general_config
        .prover_config
        .expect("ProverConfig doesn't exist");
    let keystore =
        Keystore::locate().with_setup_path(Some(prover_config.setup_data_path.clone().into()));

    setup_crs_keys(&config);

    // tracing::info!("Loading setup data from disk...");
    // let setup_start_time = Instant::now();

    // let setup_data_cache = if is_fflonk {
    //     SnarkWrapperSetup::FFfonk(
    //         keystore
    //             .get_full_fflonk_setup_data()
    //             .context("failed to get setup data for Plonk")?,
    //     )
    // } else {
    //     SnarkWrapperSetup::Plonk(
    //         keystore
    //             .get_full_plonk_setup_data()
    //             .context("failed to get setup data for Plonk")?,
    //     )
    // };

    // tracing::info!(
    //     "Finished loading mappings from disk in {:?}.",
    //     setup_start_time.elapsed()
    // );

    PROOF_FRI_COMPRESSOR_INSTANCE_METRICS
        .startup_time
        .set(start_time.elapsed());

    let cancellation_token = CancellationToken::new();

    let exporter_config = PrometheusExporterConfig::pull(prover_config.prometheus_port);
    let (metrics_stop_sender, metrics_stop_receiver) = tokio::sync::watch::channel(false);

    let mut tasks = vec![tokio::spawn(exporter_config.run(metrics_stop_receiver))];

    let proof_fri_compressor_runner = proof_fri_compressor_runner(
        pool,
        blob_store,
        protocol_version,
        is_fflonk,
        cancellation_token.clone(),
        Arc::new(keystore),
    );

    tracing::info!("Starting proof compressor");

    tasks.extend(proof_fri_compressor_runner.run());

    let mut tasks = ManagedTasks::new(tasks);
    tokio::select! {
        _ = tasks.wait_single() => {},
        result = tokio::signal::ctrl_c() => {
            match result {
                Ok(_) => {
                    tracing::info!("Stop signal received, shutting down...");
                    cancellation_token.cancel();
                },
                Err(_err) => {
                    tracing::error!("failed to set up ctrl c listener");
                }
            }
        }
    }
    let shutdown_time = Instant::now();
    metrics_stop_sender
        .send(true)
        .context("failed to stop metrics")?;
    tasks.complete(GRACEFUL_SHUTDOWN_DURATION).await;
    tracing::info!("Tasks completed in {:?}.", shutdown_time.elapsed());
    Ok(())
}

fn setup_crs_keys(config: &FriProofCompressorConfig) {
    download_initial_setup_keys_if_not_present(
        &config.universal_setup_path,
        &config.universal_setup_download_url,
    );
    std::env::set_var("COMPACT_CRS_FILE", &config.universal_setup_path);
}
