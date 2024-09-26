use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use clap::Parser;
use tokio_util::sync::CancellationToken;
use zksync_circuit_prover::{
    Backoff, CircuitProver, FinalizationHintsCache, SetupDataCache, WitnessVectorGenerator,
    PROVER_BINARY_METRICS,
};
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

#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version)]
struct Cli {
    #[arg(long)]
    pub(crate) config_path: Option<PathBuf>,
    #[arg(long)]
    pub(crate) secrets_path: Option<PathBuf>,
    /// Number of WVG jobs to run in parallel.
    /// Default value is 1.
    #[arg(long, default_value_t = 1)]
    pub(crate) witness_vector_generator_count: usize,
    /// Max VRAM to allocate. Useful if you want to limit the size of VRAM used.
    /// None corresponds to allocating all available VRAM.
    #[arg(long)]
    pub(crate) max_allocation: Option<usize>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let time = Instant::now();
    let opt = Cli::parse();

    let (observability_config, prover_config, object_store_config) = load_configs(opt.config_path)?;

    let _observability_guard = observability_config
        .install()
        .context("failed to install observability")?;

    let wvg_count = opt.witness_vector_generator_count as u32;

    let (connection_pool, object_store, setup_data_cache, hints) = load_resources(
        opt.secrets_path,
        object_store_config,
        prover_config.setup_data_path.into(),
        wvg_count,
    )
    .await
    .context("failed to load configs")?;

    PROVER_BINARY_METRICS.start_up.observe(time.elapsed());

    let cancellation_token = CancellationToken::new();
    let backoff = Backoff::new(Duration::from_secs(5), Duration::from_secs(30));

    let mut tasks = vec![];

    let (sender, receiver) = tokio::sync::mpsc::channel(5);

    tracing::info!("Starting {wvg_count} Witness Vector Generators.");

    for _ in 0..wvg_count {
        let wvg = WitnessVectorGenerator::new(
            object_store.clone(),
            connection_pool.clone(),
            PROVER_PROTOCOL_SEMANTIC_VERSION,
            sender.clone(),
            hints.clone(),
        );
        tasks.push(tokio::spawn(
            wvg.run(cancellation_token.clone(), backoff.clone()),
        ));
    }

    // NOTE: Prover Context is the way VRAM is allocated. If it is dropped, the claim on VRAM allocation is dropped as well.
    // It has to be kept until prover dies. Whilst it may be kept in prover struct, during cancellation, prover can `drop`, but the thread doing the processing can still be alive.
    // This setup prevents segmentation faults and other nasty behavior during shutdown.
    let (prover, _prover_context) = CircuitProver::new(
        connection_pool,
        object_store,
        PROVER_PROTOCOL_SEMANTIC_VERSION,
        receiver,
        opt.max_allocation,
        setup_data_cache,
    )
    .context("failed to create circuit prover")?;
    tasks.push(tokio::spawn(prover.run(cancellation_token.clone())));

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
    PROVER_BINARY_METRICS.run_time.observe(time.elapsed());
    tasks.complete(Duration::from_secs(5)).await;

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
/// - setup data - necessary for circuit proving
/// - finalization hints - necessary for generating witness vectors
async fn load_resources(
    secrets_path: Option<PathBuf>,
    object_store_config: ObjectStoreConfig,
    setup_data_path: PathBuf,
    wvg_count: u32,
) -> anyhow::Result<(
    ConnectionPool<Prover>,
    Arc<dyn ObjectStore>,
    SetupDataCache,
    FinalizationHintsCache,
)> {
    let database_secrets =
        load_database_secrets(secrets_path).context("failed to load database secrets")?;
    let database_url = database_secrets
        .prover_url
        .context("no prover DB URl present")?;

    // 1 connection for the prover and one for each vector generator
    let max_connections = 1 + wvg_count;
    let connection_pool = ConnectionPool::<Prover>::builder(database_url, max_connections)
        .build()
        .await
        .context("failed to build connection pool")?;

    let object_store = ObjectStoreFactory::new(object_store_config)
        .create_store()
        .await
        .context("failed to create object store")?;

    tracing::info!("Loading mappings from disk...");

    let keystore = Keystore::locate().with_setup_path(Some(setup_data_path));
    let setup_data_cache = keystore
        .load_all_setup_key_mapping()
        .await
        .context("failed to load setup key mapping")?;
    let finalization_hints = keystore
        .load_all_finalization_hints_mapping()
        .await
        .context("failed to load finalization hints mapping")?;

    tracing::info!("Loaded mappings from disk.");

    Ok((
        connection_pool,
        object_store,
        setup_data_cache,
        finalization_hints,
    ))
}
