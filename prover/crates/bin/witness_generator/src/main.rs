#![allow(incomplete_features)] // We have to use generic const exprs.
#![feature(generic_const_exprs)]

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context as _};
#[cfg(not(target_env = "msvc"))]
use jemallocator::Jemalloc;
use structopt::StructOpt;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use zksync_config::{
    configs::{DatabaseSecrets, GeneralConfig},
    full_config_schema,
    sources::ConfigFilePaths,
};
use zksync_object_store::ObjectStoreFactory;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::PROVER_PROTOCOL_SEMANTIC_VERSION;
use zksync_prover_keystore::keystore::Keystore;
use zksync_task_management::ManagedTasks;
use zksync_types::{basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion};
use zksync_vlog::prometheus::PrometheusExporterConfig;
use zksync_witness_generator::metrics::SERVER_METRICS;
use zksync_witness_generator_service::{
    rounds::{BasicCircuits, LeafAggregation, NodeAggregation, RecursionTip, Scheduler},
    witness_generator_runner,
};

const GRACEFUL_SHUTDOWN_DURATION: Duration = Duration::from_secs(20);

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "Run witness generator for different aggregation round",
    about = "Component for generating witness"
)]
struct Opt {
    /// Number of times witness generator should be run.
    #[structopt(short = "b", long = "batch_size")]
    batch_size: Option<usize>,
    /// Aggregation rounds options, they can be run individually or together.
    ///
    /// Single aggregation round for the witness generator.
    #[structopt(short = "r", long = "round")]
    round: Option<AggregationRound>,
    /// Start all aggregation rounds for the witness generator.
    #[structopt(short = "a", long = "all_rounds")]
    all_rounds: bool,
    /// Path to the configuration file.
    #[structopt(long)]
    config_path: Option<std::path::PathBuf>,
    /// Path to the secrets file.
    #[structopt(long)]
    secrets_path: Option<std::path::PathBuf>,
}

/// Checks if the configuration locally matches the one in the database.
/// This function recalculates the commitment in order to check the exact code that
/// will run, instead of loading `commitments.json` (which also may correct misaligned
/// information).
async fn ensure_protocol_alignment(
    prover_pool: &ConnectionPool<Prover>,
    protocol_version: ProtocolSemanticVersion,
    keystore: &Keystore,
) -> anyhow::Result<()> {
    tracing::info!("Verifying protocol alignment for {:?}", protocol_version);
    let vk_commitments_in_db = match prover_pool
        .connection()
        .await
        .unwrap()
        .fri_protocol_versions_dal()
        .vk_commitments_for(protocol_version)
        .await
    {
        Some(commitments) => commitments,
        None => {
            panic!(
                "No vk commitments available in database for a protocol version {:?}.",
                protocol_version
            );
        }
    };
    let scheduler_vk_hash = vk_commitments_in_db.snark_wrapper_vk_hash;
    keystore
        .verify_scheduler_vk_hash(scheduler_vk_hash)
        .with_context(||
            format!("VK commitments didn't match commitments from DB for protocol version {protocol_version:?}")
        )?;
    Ok(())
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
    let opt = Opt::from_args();
    let schema = full_config_schema(false);
    let config_file_paths = ConfigFilePaths {
        general: opt.config_path,
        secrets: opt.secrets_path,
        ..ConfigFilePaths::default()
    };
    let config_sources = config_file_paths.into_config_sources("ZKSYNC_")?;

    let _observability_guard = config_sources.observability()?.install()?;

    let mut repo = config_sources.build_repository(&schema);
    let general_config: GeneralConfig = repo.parse()?;
    let database_secrets: DatabaseSecrets = repo.parse()?;

    let prover_config = general_config.prover_config.context("prover config")?;
    let object_store_config = prover_config.prover_object_store;
    let store_factory = ObjectStoreFactory::new(object_store_config);
    let config = general_config
        .witness_generator_config
        .context("witness generator config")?
        .clone();
    let keystore = Keystore::locate().with_setup_path(Some(prover_config.setup_data_path));

    let prometheus_exporter_config = general_config
        .prometheus_config
        .build_exporter_config(config.prometheus_listener_port)
        .context("Failed to build Prometheus exporter configuration")?;
    tracing::info!("Using Prometheus exporter with {prometheus_exporter_config:?}");

    let connection_pool = ConnectionPool::<Prover>::singleton(database_secrets.prover_url()?)
        .build()
        .await
        .context("failed to build a prover_connection_pool")?;

    let protocol_version = PROVER_PROTOCOL_SEMANTIC_VERSION;

    ensure_protocol_alignment(&connection_pool, protocol_version, &keystore)
        .await
        .unwrap_or_else(|err| panic!("Protocol alignment check failed: {:?}", err));

    let rounds = match (opt.round, opt.all_rounds) {
        (Some(round), false) => vec![round],
        (None, true) => vec![
            AggregationRound::BasicCircuits,
            AggregationRound::LeafAggregation,
            AggregationRound::NodeAggregation,
            AggregationRound::RecursionTip,
            AggregationRound::Scheduler,
        ],
        (Some(_), true) => {
            return Err(anyhow!(
                "Cannot set both the --all_rounds and --round flags. Choose one or the other."
            ));
        }
        (None, false) => {
            return Err(anyhow!(
                "Expected --all_rounds flag with no --round flag present"
            ));
        }
    };

    let mut tasks = vec![tokio::spawn(
        prometheus_exporter_config.run(metrics_stop_receiver),
    )];

    let keystore = Arc::new(keystore);

    for round in rounds {
        tracing::info!(
            "initializing the {:?} witness generator, batch size: {:?} with protocol_version: {:?}",
            round,
            opt.batch_size,
            &protocol_version
        );

        let witness_generator_tasks = match round {
            AggregationRound::BasicCircuits => {
                let runner = witness_generator_runner::<BasicCircuits>(
                    config.max_circuits_in_flight.clone(),
                    store_factory.create_store().await?,
                    connection_pool.clone(),
                    1,
                    protocol_version,
                    keystore.clone(),
                    cancellation_token.clone(),
                );
                runner.run()
            }
            AggregationRound::LeafAggregation => {
                let runner = witness_generator_runner::<LeafAggregation>(
                    config.max_circuits_in_flight.clone(),
                    store_factory.create_store().await?,
                    connection_pool.clone(),
                    1,
                    protocol_version,
                    keystore.clone(),
                    cancellation_token.clone(),
                );
                runner.run()
            }
            AggregationRound::NodeAggregation => {
                let runner = witness_generator_runner::<NodeAggregation>(
                    config.max_circuits_in_flight.clone(),
                    store_factory.create_store().await?,
                    connection_pool.clone(),
                    1,
                    protocol_version,
                    keystore.clone(),
                    cancellation_token.clone(),
                );
                runner.run()
            }
            AggregationRound::RecursionTip => {
                let runner = witness_generator_runner::<RecursionTip>(
                    config.max_circuits_in_flight.clone(),
                    store_factory.create_store().await?,
                    connection_pool.clone(),
                    1,
                    protocol_version,
                    keystore.clone(),
                    cancellation_token.clone(),
                );
                runner.run()
            }
            AggregationRound::Scheduler => {
                let runner = witness_generator_runner::<Scheduler>(
                    config.max_circuits_in_flight.clone(),
                    store_factory.create_store().await?,
                    connection_pool.clone(),
                    1,
                    protocol_version,
                    keystore.clone(),
                    cancellation_token.clone(),
                );
                runner.run()
            }
        };

        tasks.extend(witness_generator_tasks);

        tracing::info!(
            "initialized {:?} witness generator in {:?}",
            round,
            started_at.elapsed()
        );
        SERVER_METRICS.init_latency[&round.into()].set(started_at.elapsed());
    }

    *managed_tasks = ManagedTasks::new(tasks);
    managed_tasks.wait_single().await;
    Ok(())
}
