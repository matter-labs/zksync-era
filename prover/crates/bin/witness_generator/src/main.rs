#![allow(incomplete_features)] // We have to use generic const exprs.
#![feature(generic_const_exprs)]

use std::time::{Duration, Instant};

use anyhow::{anyhow, Context as _};
use futures::{channel::mpsc, executor::block_on, SinkExt, StreamExt};
#[cfg(not(target_env = "msvc"))]
use jemallocator::Jemalloc;
use structopt::StructOpt;
use tokio::sync::watch;
use zksync_core_leftovers::temp_config_store::{load_database_secrets, load_general_config};
use zksync_env_config::object_store::ProverObjectStoreConfig;
use zksync_object_store::ObjectStoreFactory;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_fri_types::PROVER_PROTOCOL_SEMANTIC_VERSION;
use zksync_prover_keystore::keystore::Keystore;
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion};
use zksync_utils::wait_for_tasks::ManagedTasks;
use zksync_vlog::prometheus::PrometheusExporterConfig;
use zksync_witness_generator::{
    basic_circuits::BasicWitnessGenerator, leaf_aggregation::LeafAggregationWitnessGenerator,
    metrics::SERVER_METRICS, node_aggregation::NodeAggregationWitnessGenerator,
    recursion_tip::RecursionTipWitnessGenerator, scheduler::SchedulerWitnessGenerator,
};

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
    let opt = Opt::from_args();

    let general_config = load_general_config(opt.config_path).context("general config")?;

    let database_secrets = load_database_secrets(opt.secrets_path).context("database secrets")?;

    let observability_config = general_config
        .observability
        .context("observability config")?;
    let _observability_guard = observability_config.install()?;

    let started_at = Instant::now();
    let use_push_gateway = opt.batch_size.is_some();

    let prover_config = general_config.prover_config.context("prover config")?;
    let object_store_config = ProverObjectStoreConfig(
        prover_config
            .prover_object_store
            .context("object store")?
            .clone(),
    );
    let store_factory = ObjectStoreFactory::new(object_store_config.0);
    let config = general_config
        .witness_generator_config
        .context("witness generator config")?
        .clone();
    let keystore =
        Keystore::locate().with_setup_path(Some(prover_config.setup_data_path.clone().into()));

    let prometheus_config = general_config.prometheus_config.clone();

    // If the prometheus listener port is not set in the witness generator config, use the one from the prometheus config.
    let prometheus_listener_port = if let Some(port) = config.prometheus_listener_port {
        port
    } else {
        prometheus_config
            .clone()
            .context("prometheus config")?
            .listener_port
    };

    let prover_connection_pool =
        ConnectionPool::<Prover>::singleton(database_secrets.prover_url()?)
            .build()
            .await
            .context("failed to build a prover_connection_pool")?;
    let (stop_sender, stop_receiver) = watch::channel(false);

    let protocol_version = PROVER_PROTOCOL_SEMANTIC_VERSION;
    ensure_protocol_alignment(&prover_connection_pool, protocol_version, &keystore)
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

    let prometheus_config = if use_push_gateway {
        let prometheus_config = prometheus_config
            .clone()
            .context("prometheus config needed when use_push_gateway enabled")?;
        PrometheusExporterConfig::push(
            prometheus_config
                .gateway_endpoint()
                .context("gateway_endpoint needed when use_push_gateway enabled")?,
            prometheus_config.push_interval(),
        )
    } else {
        PrometheusExporterConfig::pull(prometheus_listener_port as u16)
    };
    let prometheus_task = prometheus_config.run(stop_receiver.clone());

    let mut tasks = Vec::new();
    tasks.push(tokio::spawn(prometheus_task));

    for round in rounds {
        tracing::info!(
            "initializing the {:?} witness generator, batch size: {:?} with protocol_version: {:?}",
            round,
            opt.batch_size,
            &protocol_version
        );

        let witness_generator_task = match round {
            AggregationRound::BasicCircuits => {
                let public_blob_store = match config.shall_save_to_public_bucket {
                    false => None,
                    true => Some(
                        ObjectStoreFactory::new(
                            prover_config
                                .public_object_store
                                .clone()
                                .expect("public_object_store"),
                        )
                        .create_store()
                        .await?,
                    ),
                };
                let generator = BasicWitnessGenerator::new(
                    config.clone(),
                    store_factory.create_store().await?,
                    public_blob_store,
                    prover_connection_pool.clone(),
                    protocol_version,
                );
                generator.run(stop_receiver.clone(), opt.batch_size)
            }
            AggregationRound::LeafAggregation => {
                let generator = LeafAggregationWitnessGenerator::new(
                    config.clone(),
                    store_factory.create_store().await?,
                    prover_connection_pool.clone(),
                    protocol_version,
                    keystore.clone(),
                );
                generator.run(stop_receiver.clone(), opt.batch_size)
            }
            AggregationRound::NodeAggregation => {
                let generator = NodeAggregationWitnessGenerator::new(
                    config.clone(),
                    store_factory.create_store().await?,
                    prover_connection_pool.clone(),
                    protocol_version,
                    keystore.clone(),
                );
                generator.run(stop_receiver.clone(), opt.batch_size)
            }
            AggregationRound::RecursionTip => {
                let generator = RecursionTipWitnessGenerator::new(
                    config.clone(),
                    store_factory.create_store().await?,
                    prover_connection_pool.clone(),
                    protocol_version,
                    keystore.clone(),
                );
                generator.run(stop_receiver.clone(), opt.batch_size)
            }
            AggregationRound::Scheduler => {
                let generator = SchedulerWitnessGenerator::new(
                    config.clone(),
                    store_factory.create_store().await?,
                    prover_connection_pool.clone(),
                    protocol_version,
                    keystore.clone(),
                );
                generator.run(stop_receiver.clone(), opt.batch_size)
            }
        };

        tasks.push(tokio::spawn(witness_generator_task));

        tracing::info!(
            "initialized {:?} witness generator in {:?}",
            round,
            started_at.elapsed()
        );
        SERVER_METRICS.init_latency[&round.into()].set(started_at.elapsed());
    }

    let (mut stop_signal_sender, mut stop_signal_receiver) = mpsc::channel(256);
    ctrlc::set_handler(move || {
        block_on(stop_signal_sender.send(true)).expect("Ctrl+C signal send");
    })
    .expect("Error setting Ctrl+C handler");
    let mut tasks = ManagedTasks::new(tasks).allow_tasks_to_finish();
    tokio::select! {
        _ = tasks.wait_single() => {},
        _ = stop_signal_receiver.next() => {
            tracing::info!("Stop signal received, shutting down");
        }
    }

    stop_sender.send_replace(true);
    tasks.complete(Duration::from_secs(5)).await;
    tracing::info!("Finished witness generation");
    Ok(())
}
