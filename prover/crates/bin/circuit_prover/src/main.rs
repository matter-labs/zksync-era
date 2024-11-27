use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use clap::Parser;
use shivini::{ProverContext, ProverContextConfig};
use tokio_util::sync::CancellationToken;
use zksync_circuit_prover::{FinalizationHintsCache, SetupDataCache, PROVER_BINARY_METRICS};
use zksync_circuit_prover_service::job_runner::{circuit_prover_runner, WvgRunnerBuilder};
use zksync_config::{
    configs::{FriProverConfig, ObservabilityConfig},
    ObjectStoreConfig,
};
use zksync_core_leftovers::temp_config_store::{load_database_secrets, load_general_config};
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_fri_types::PROVER_PROTOCOL_SEMANTIC_VERSION;
use zksync_prover_keystore::keystore::Keystore;
use zksync_utils::wait_for_tasks::ManagedTasks;
use zksync_vlog::prometheus::PrometheusExporterConfig;

/// On most commodity hardware, WVG can take ~30 seconds to complete.
/// GPU processing is ~1 second.
/// Typical setup is ~25 WVGs & 1 GPU.
/// Worst case scenario, you just picked all 25 WVGs (so you need 30 seconds to finish)
/// and another 25 for the GPU.
const GRACEFUL_SHUTDOWN_DURATION: Duration = Duration::from_secs(55);

/// With current setup, only a single job is expected to be in flight.
/// This guarantees memory consumption is going to be fixed (1 job in memory, no more).
/// Additionally, helps with estimating graceful shutdown time.
/// Free side effect, if the machine dies, only 1 job is in "pending" state.
const CHANNEL_SIZE: usize = 1;

#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version)]
struct Cli {
    /// Path to file configuration
    #[arg(short = 'c', long)]
    pub(crate) config_path: Option<PathBuf>,
    /// Path to file secrets
    #[arg(short = 's', long)]
    pub(crate) secrets_path: Option<PathBuf>,
    /// Number of light witness vector generators to run in parallel.
    /// Corresponds to 1 CPU thread & ~2GB of RAM.
    #[arg(short = 'l', long, default_value_t = 1)]
    light_wvg_count: usize,
    /// Number of heavy witness vector generators to run in parallel.
    /// Corresponds to 1 CPU thread & ~9GB of RAM.
    #[arg(short = 'h', long, default_value_t = 1)]
    heavy_wvg_count: usize,
    /// Max VRAM to allocate. Useful if you want to limit the size of VRAM used.
    /// None corresponds to allocating all available VRAM.
    #[arg(short = 'm', long)]
    pub(crate) max_allocation: Option<usize>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let start_time = Instant::now();
    let opt = Cli::parse();

    let (observability_config, prover_config, object_store_config) = load_configs(opt.config_path)?;
    let _observability_guard = observability_config
        .install()
        .context("failed to install observability")?;

    let (connection_pool, object_store, prover_context, setup_data_cache, hints) = load_resources(
        opt.secrets_path,
        opt.max_allocation,
        object_store_config,
        prover_config.setup_data_path.into(),
    )
    .await
    .context("failed to load configs")?;

    PROVER_BINARY_METRICS
        .startup_time
        .observe(start_time.elapsed());

    let cancellation_token = CancellationToken::new();

    let exporter_config = PrometheusExporterConfig::pull(prover_config.prometheus_port);
    let (metrics_stop_sender, metrics_stop_receiver) = tokio::sync::watch::channel(false);

    let mut tasks = vec![tokio::spawn(exporter_config.run(metrics_stop_receiver))];

    let (witness_vector_sender, witness_vector_receiver) = tokio::sync::mpsc::channel(CHANNEL_SIZE);

    tracing::info!(
        "Starting {} light WVGs and {} heavy WVGs.",
        opt.light_wvg_count,
        opt.heavy_wvg_count
    );

    let builder = WvgRunnerBuilder::new(
        connection_pool.clone(),
        object_store.clone(),
        PROVER_PROTOCOL_SEMANTIC_VERSION,
        hints.clone(),
        witness_vector_sender,
        cancellation_token.clone(),
    );

    let light_wvg_runner = builder.light_wvg_runner(opt.light_wvg_count);
    let heavy_wvg_runner = builder.heavy_wvg_runner(opt.heavy_wvg_count);

    tasks.extend(light_wvg_runner.run());
    tasks.extend(heavy_wvg_runner.run());

    // necessary as it has a connection_pool which will keep 1 connection active by default
    drop(builder);

    let circuit_prover_runner = circuit_prover_runner(
        connection_pool,
        object_store,
        PROVER_PROTOCOL_SEMANTIC_VERSION,
        setup_data_cache,
        witness_vector_receiver,
        prover_context,
    );

    tasks.extend(circuit_prover_runner.run());

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
    tasks.complete(GRACEFUL_SHUTDOWN_DURATION).await;
    PROVER_BINARY_METRICS
        .shutdown_time
        .observe(shutdown_time.elapsed());
    PROVER_BINARY_METRICS.run_time.observe(start_time.elapsed());
    metrics_stop_sender
        .send(true)
        .context("failed to stop metrics")?;
    Ok(())
}
/// Loads configs necessary for proving.
/// - observability config - for observability setup
/// - prover config - necessary for setup data
/// - object store config - for retrieving artifacts for WVG & CP
fn load_configs(
    config_path: Option<PathBuf>,
) -> anyhow::Result<(ObservabilityConfig, FriProverConfig, ObjectStoreConfig)> {
    tracing::info!("loading configs...");
    let general_config =
        load_general_config(config_path).context("failed loading general config")?;
    let observability_config = general_config
        .observability
        .context("failed loading observability config")?;
    let prover_config = general_config
        .prover_config
        .context("failed loading prover config")?;
    let object_store_config = prover_config
        .prover_object_store
        .clone()
        .context("failed loading prover object store config")?;
    tracing::info!("Loaded configs.");
    Ok((observability_config, prover_config, object_store_config))
}
/// Loads resources necessary for proving.
/// - connection pool - necessary to pick & store jobs from database
/// - object store - necessary  for loading and storing artifacts to object store
/// - prover context - necessary for circuit proving; VRAM allocation
/// - setup data - necessary for circuit proving
/// - finalization hints - necessary for generating witness vectors
async fn load_resources(
    secrets_path: Option<PathBuf>,
    max_gpu_vram_allocation: Option<usize>,
    object_store_config: ObjectStoreConfig,
    setup_data_path: PathBuf,
) -> anyhow::Result<(
    ConnectionPool<Prover>,
    Arc<dyn ObjectStore>,
    ProverContext,
    SetupDataCache,
    FinalizationHintsCache,
)> {
    let database_secrets =
        load_database_secrets(secrets_path).context("failed to load database secrets")?;
    let database_url = database_secrets
        .prover_url
        .context("no prover DB URl present")?;
    // 2 connections for the witness vector generator job pickers (1 each) and 1 for gpu circuit prover job saver
    let max_connections = 3;
    let connection_pool = ConnectionPool::<Prover>::builder(database_url, max_connections)
        .build()
        .await
        .context("failed to build connection pool")?;

    let object_store = ObjectStoreFactory::new(object_store_config)
        .create_store()
        .await
        .context("failed to create object store")?;

    let prover_context = match max_gpu_vram_allocation {
        Some(max_allocation) => ProverContext::create_with_config(
            ProverContextConfig::default().with_maximum_device_allocation(max_allocation),
        )
        .context("failed initializing fixed gpu prover context")?,
        None => ProverContext::create().context("failed initializing gpu prover context")?,
    };

    tracing::info!("Loading setup data from disk...");

    let keystore = Keystore::locate().with_setup_path(Some(setup_data_path));
    let setup_data_cache = keystore
        .load_all_setup_key_mapping()
        .await
        .context("failed to load setup key mapping")?;

    tracing::info!("Loading finalization hints from disk...");
    let finalization_hints = keystore
        .load_all_finalization_hints_mapping()
        .await
        .context("failed to load finalization hints mapping")?;

    tracing::info!("Finished loading mappings from disk.");

    Ok((
        connection_pool,
        object_store,
        prover_context,
        setup_data_cache,
        finalization_hints,
    ))
}
