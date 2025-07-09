use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use clap::Parser;
use shivini::{ProverContext, ProverContextConfig};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use zksync_circuit_prover::{FinalizationHintsCache, SetupDataCache, PROVER_BINARY_METRICS};
use zksync_circuit_prover_service::job_runner::{circuit_prover_runner, WvgRunnerBuilder};
use zksync_config::{
    configs::{GeneralConfig, PostgresSecrets},
    full_config_schema,
    sources::ConfigFilePaths,
    ObjectStoreConfig,
};
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_fri_types::PROVER_PROTOCOL_SEMANTIC_VERSION;
use zksync_prover_keystore::keystore::Keystore;
use zksync_task_management::ManagedTasks;

/// On most commodity hardware, WVG can take ~30 seconds to complete.
/// GPU processing is ~1 second.
/// Typical setup is ~25 WVGs & 1 GPU.
/// Worst case scenario, you just picked all 25 WVGs (so you need 30 seconds to finish)
/// and another 25 for the GPU.
const GRACEFUL_SHUTDOWN_DURATION: Duration = Duration::from_secs(70);

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
        res = run_inner(cancellation_token.clone(), metrics_stop_receiver, &mut managed_tasks) => {
            res?
        },
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

    let prover_config = general_config
        .prover_config
        .context("failed loading prover config")?;
    let object_store_config = prover_config.prover_object_store.clone();
    tracing::info!("Loaded configs.");

    let prometheus_exporter_config = general_config
        .prometheus_config
        .build_exporter_config(prover_config.prometheus_port)
        .context("Failed to build Prometheus exporter configuration")?;
    tracing::info!("Using Prometheus exporter with {prometheus_exporter_config:?}");

    let mut tasks = vec![tokio::spawn(
        prometheus_exporter_config.run(metrics_stop_receiver),
    )];

    let (connection_pool, object_store, prover_context, setup_data_cache, hints) = load_resources(
        database_secrets,
        opt.max_allocation,
        object_store_config,
        prover_config.setup_data_path,
    )
    .await
    .context("failed to load configs")?;

    let (witness_vector_sender, witness_vector_receiver) = tokio::sync::mpsc::channel(CHANNEL_SIZE);

    PROVER_BINARY_METRICS.startup_time.set(start_time.elapsed());
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

    *managed_tasks = ManagedTasks::new(tasks);
    managed_tasks.wait_single().await;
    Ok(())
}

/// Loads resources necessary for proving.
/// - connection pool - necessary to pick & store jobs from database
/// - object store - necessary  for loading and storing artifacts to object store
/// - prover context - necessary for circuit proving; VRAM allocation
/// - setup data - necessary for circuit proving
/// - finalization hints - necessary for generating witness vectors
async fn load_resources(
    database_secrets: PostgresSecrets,
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
