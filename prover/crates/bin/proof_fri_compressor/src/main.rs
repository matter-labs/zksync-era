#![allow(incomplete_features)] // We have to use generic const exprs.
#![feature(generic_const_exprs)]

use std::time::Duration;

use anyhow::Context as _;
use clap::Parser;
use tokio::sync::{oneshot, watch};
use zksync_config::{
    configs::{DatabaseSecrets, FriProofCompressorConfig, GeneralConfig},
    full_config_schema,
    sources::ConfigFilePaths,
    ConfigRepositoryExt,
};
use zksync_object_store::ObjectStoreFactory;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::PROVER_PROTOCOL_SEMANTIC_VERSION;
use zksync_prover_keystore::keystore::Keystore;
use zksync_queued_job_processor::JobProcessor;
use zksync_task_management::ManagedTasks;
use zksync_vlog::prometheus::PrometheusExporterConfig;

use crate::{
    compressor::ProofCompressor, initial_setup_keys::download_initial_setup_keys_if_not_present,
};

mod compressor;
mod initial_setup_keys;
mod metrics;

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
    let opt = Cli::parse();
    let is_fflonk = opt.fflonk.unwrap_or(false);
    let schema = full_config_schema(false);
    let config_file_paths = ConfigFilePaths {
        general: opt.config_path,
        secrets: opt.secrets_path,
        ..ConfigFilePaths::default()
    };
    let config_sources = config_file_paths.into_config_sources("ZKSYNC_")?;

    let _observability_guard = config_sources.observability()?.install()?;

    let repo = config_sources.build_repository(&schema);
    let general_config: GeneralConfig = repo.parse()?;
    let database_secrets: DatabaseSecrets = repo.parse()?;

    let config = general_config
        .proof_compressor_config
        .context("FriProofCompressorConfig")?;
    let pool = ConnectionPool::<Prover>::singleton(database_secrets.prover_url()?)
        .build()
        .await
        .context("failed to build a connection pool")?;

    let prover_config = general_config
        .prover_config
        .context("ProverConfig doesn't exist")?;
    let object_store_config = prover_config.prover_object_store;
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

    let proof_compressor = ProofCompressor::new(
        blob_store,
        pool,
        config.max_attempts,
        protocol_version,
        keystore,
        is_fflonk,
    );

    let (stop_sender, stop_receiver) = watch::channel(false);

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(stop_signal_sender) = stop_signal_sender.take() {
            stop_signal_sender.send(()).ok();
        }
    })
    .expect("Error setting Ctrl+C handler"); // Setting handler should always succeed.

    setup_crs_keys(&config);

    tracing::info!("Starting proof compressor");

    let prometheus_config = PrometheusExporterConfig::push(
        config.prometheus_pushgateway_url,
        config.prometheus_push_interval_ms,
    );
    let tasks = vec![
        tokio::spawn(prometheus_config.run(stop_receiver.clone())),
        tokio::spawn(proof_compressor.run(stop_receiver, opt.number_of_iterations)),
    ];

    let mut tasks = ManagedTasks::new(tasks).allow_tasks_to_finish();
    tokio::select! {
        _ = tasks.wait_single() => {},
        _ = stop_signal_receiver => {
            tracing::info!("Stop request received, shutting down");
        }
    };
    stop_sender.send_replace(true);
    tasks.complete(Duration::from_secs(5)).await;
    Ok(())
}

fn setup_crs_keys(config: &FriProofCompressorConfig) {
    download_initial_setup_keys_if_not_present(
        &config.universal_setup_path,
        &config.universal_setup_download_url,
    );
    std::env::set_var("COMPACT_CRS_FILE", &config.universal_setup_path);
}
