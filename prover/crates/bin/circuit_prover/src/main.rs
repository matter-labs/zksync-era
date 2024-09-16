use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};

use anyhow::Context as _;
use clap::Parser;
use tokio_util::sync::CancellationToken;
use zksync_circuit_prover::{CircuitProver, WitnessVectorGenerator};
use zksync_config::{
    configs::{FriProverConfig, ObservabilityConfig},
    ObjectStoreConfig,
};
use zksync_core_leftovers::temp_config_store::{load_database_secrets, load_general_config};
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_fri_types::{
    circuit_definitions::boojum::cs::implementations::setup::FinalizationHintsForProver,
    ProverServiceDataKey, PROVER_PROTOCOL_SEMANTIC_VERSION,
};
use zksync_prover_keystore::{keystore::Keystore, GoldilocksGpuProverSetupData};
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
    #[arg(long)]
    pub(crate) max_allocation: Option<usize>,
}
//
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Cli::parse();

    let (observability_config, prover_config, object_store_config) = load_configs(opt.config_path)?;

    let _observability_guard = observability_config.install()?;

    let wvg_count = opt.witness_vector_generator_count as u32;

    let (connection_pool, object_store, setup_keys, hints) = load_resources(
        opt.secrets_path,
        object_store_config,
        prover_config.setup_data_path.into(),
        wvg_count,
    )
    .await?;

    let cancellation_token = CancellationToken::new();

    let mut tasks = vec![];

    // TODO: Add config value here
    let (sender, receiver) = tokio::sync::mpsc::channel(5);

    for _ in 0..wvg_count {
        let wvg = WitnessVectorGenerator::new(
            object_store.clone(),
            connection_pool.clone(),
            PROVER_PROTOCOL_SEMANTIC_VERSION,
            prover_config.max_attempts,
            hints.clone(),
            sender.clone(),
        );
        tasks.push(tokio::spawn(wvg.run(cancellation_token.clone())));
    }

    let prover = CircuitProver::new(
        connection_pool,
        object_store,
        PROVER_PROTOCOL_SEMANTIC_VERSION,
        receiver,
        opt.max_allocation,
        setup_keys,
    );
    tasks.push(tokio::spawn(prover.run(cancellation_token.clone())));

    let mut tasks = ManagedTasks::new(tasks);
    tokio::select! {
        _ = tasks.wait_single() => {},
        result = tokio::signal::ctrl_c() => {
            match result {
                Ok(_) => {
                    tracing::info!("Stop signal received, shutting down");
                    cancellation_token.cancel();
                },
                Err(_err) => {
                    tracing::error!("failed to set up ctrl c listener");
                }
            }
        }
    }
    tasks.complete(Duration::from_secs(5)).await;

    Ok(())
}

fn load_configs(
    config_path: Option<PathBuf>,
) -> anyhow::Result<(ObservabilityConfig, FriProverConfig, ObjectStoreConfig)> {
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
    Ok((observability_config, prover_config, object_store_config))
}

async fn load_resources(
    secrets_path: Option<PathBuf>,
    object_store_config: ObjectStoreConfig,
    setup_data_path: PathBuf,
    wvg_count: u32,
) -> anyhow::Result<(
    ConnectionPool<Prover>,
    Arc<dyn ObjectStore>,
    HashMap<ProverServiceDataKey, Arc<GoldilocksGpuProverSetupData>>,
    HashMap<ProverServiceDataKey, Arc<FinalizationHintsForProver>>,
)> {
    let database_secrets = load_database_secrets(secrets_path).context("database secrets")?;
    let database_url = database_secrets
        .prover_url
        .context("no prover DB URl present")?;

    // 1 connection for the prover and another one for each vector generator
    let max_connections = 1 + wvg_count;
    let connection_pool = ConnectionPool::<Prover>::builder(database_url, max_connections)
        .build()
        .await
        .context("failed to build connection pool")?;

    let object_store = ObjectStoreFactory::new(object_store_config)
        .create_store()
        .await
        .expect("failed to create object store");

    let keystore = Keystore::locate().with_setup_path(Some(setup_data_path));
    let setup_keys = keystore.load_all_setup_key_mappings().await?;
    let finalization_hints = keystore.load_all_finalization_hints_mappings().await?;
    Ok((
        connection_pool,
        object_store,
        setup_keys,
        finalization_hints,
    ))
}
