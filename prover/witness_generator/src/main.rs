#![feature(generic_const_exprs)]

use std::time::{Duration, Instant};

use anyhow::{anyhow, Context as _};
use futures::{channel::mpsc, executor::block_on, SinkExt};
use prometheus_exporter::PrometheusExporterConfig;
use prover_dal::{ConnectionPool, Prover, ProverDal};
use structopt::StructOpt;
use tokio::sync::watch;
use zksync_config::{
    configs::{FriWitnessGeneratorConfig, ObservabilityConfig, PostgresConfig, PrometheusConfig},
    ObjectStoreConfig,
};
use zksync_env_config::{object_store::ProverObjectStoreConfig, FromEnv};
use zksync_object_store::ObjectStoreFactory;
use zksync_queued_job_processor::JobProcessor;
use zksync_types::{basic_fri_types::AggregationRound, web3::futures::StreamExt};
use zksync_utils::wait_for_tasks::ManagedTasks;
use zksync_vk_setup_data_server_fri::commitment_utils::get_cached_commitments;

use crate::{
    basic_circuits::BasicWitnessGenerator, leaf_aggregation::LeafAggregationWitnessGenerator,
    metrics::SERVER_METRICS, node_aggregation::NodeAggregationWitnessGenerator,
    scheduler::SchedulerWitnessGenerator,
};

mod basic_circuits;
mod leaf_aggregation;
mod metrics;
mod node_aggregation;
mod precalculated_merkle_paths_provider;
mod scheduler;
mod storage_oracle;
mod utils;

#[cfg(not(target_env = "msvc"))]
use jemallocator::Jemalloc;
use zksync_dal::Core;

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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let observability_config =
        ObservabilityConfig::from_env().context("ObservabilityConfig::from_env()")?;
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

    // Report whether sentry is running after the logging subsystem was initialized.
    if let Some(sentry_url) = observability_config.sentry_url {
        tracing::info!("Sentry configured with URL: {sentry_url}",);
    } else {
        tracing::info!("No sentry URL was provided");
    }

    let opt = Opt::from_args();
    let started_at = Instant::now();
    let use_push_gateway = opt.batch_size.is_some();

    let object_store_config =
        ProverObjectStoreConfig::from_env().context("ProverObjectStoreConfig::from_env()")?;
    let store_factory = ObjectStoreFactory::new(object_store_config.0);
    let config =
        FriWitnessGeneratorConfig::from_env().context("FriWitnessGeneratorConfig::from_env()")?;
    let prometheus_config = PrometheusConfig::from_env().context("PrometheusConfig::from_env()")?;
    let postgres_config = PostgresConfig::from_env().context("PostgresConfig::from_env()")?;
    let connection_pool = ConnectionPool::<Core>::builder(
        postgres_config.master_url()?,
        postgres_config.max_connections()?,
    )
    .build()
    .await
    .context("failed to build a connection_pool")?;
    let prover_connection_pool = ConnectionPool::<Prover>::singleton(postgres_config.prover_url()?)
        .build()
        .await
        .context("failed to build a prover_connection_pool")?;
    let (stop_sender, stop_receiver) = watch::channel(false);
    let vk_commitments = get_cached_commitments();
    let protocol_versions = prover_connection_pool
        .connection()
        .await
        .unwrap()
        .fri_protocol_versions_dal()
        .protocol_version_for(&vk_commitments)
        .await;

    // If `batch_size` is none, it means that the job is 'looping forever' (this is the usual setup in local network).
    // At the same time, we're reading the `protocol_version` only once at startup - so if there is no protocol version
    // read (this is often due to the fact, that the gateway was started too late, and it didn't put the updated protocol
    // versions into the database) - then the job will simply 'hang forever' and not pick any tasks.
    if opt.batch_size.is_none() && protocol_versions.is_empty() {
        panic!(
            "Could not find a protocol version for my commitments. Is gateway running?  Maybe you started this job before gateway updated the database? Commitments: {:?}",
            vk_commitments
        );
    }

    let rounds = match (opt.round, opt.all_rounds) {
        (Some(round), false) => vec![round],
        (None, true) => vec![
            AggregationRound::BasicCircuits,
            AggregationRound::LeafAggregation,
            AggregationRound::NodeAggregation,
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

    let mut tasks = Vec::new();

    for (i, round) in rounds.iter().enumerate() {
        tracing::info!(
            "initializing the {:?} witness generator, batch size: {:?} with protocol_versions: {:?}",
            round,
            opt.batch_size,
            &protocol_versions
        );

        let prometheus_config = if use_push_gateway {
            PrometheusExporterConfig::push(
                prometheus_config.gateway_endpoint(),
                prometheus_config.push_interval(),
            )
        } else {
            // `u16` cast is safe since i is in range [0, 4)
            PrometheusExporterConfig::pull(prometheus_config.listener_port + i as u16)
        };
        let prometheus_task = prometheus_config.run(stop_receiver.clone());

        let witness_generator_task = match round {
            AggregationRound::BasicCircuits => {
                let public_blob_store = match config.shall_save_to_public_bucket {
                    false => None,
                    true => Some(
                        ObjectStoreFactory::new(
                            ObjectStoreConfig::from_env()
                                .context("ObjectStoreConfig::from_env()")?,
                        )
                        .create_store()
                        .await,
                    ),
                };
                let generator = BasicWitnessGenerator::new(
                    config.clone(),
                    &store_factory,
                    public_blob_store,
                    connection_pool.clone(),
                    prover_connection_pool.clone(),
                    protocol_versions.clone(),
                )
                .await;
                generator.run(stop_receiver.clone(), opt.batch_size)
            }
            AggregationRound::LeafAggregation => {
                let generator = LeafAggregationWitnessGenerator::new(
                    config.clone(),
                    &store_factory,
                    prover_connection_pool.clone(),
                    protocol_versions.clone(),
                )
                .await;
                generator.run(stop_receiver.clone(), opt.batch_size)
            }
            AggregationRound::NodeAggregation => {
                let generator = NodeAggregationWitnessGenerator::new(
                    config.clone(),
                    &store_factory,
                    prover_connection_pool.clone(),
                    protocol_versions.clone(),
                )
                .await;
                generator.run(stop_receiver.clone(), opt.batch_size)
            }
            AggregationRound::Scheduler => {
                let generator = SchedulerWitnessGenerator::new(
                    config.clone(),
                    &store_factory,
                    prover_connection_pool.clone(),
                    protocol_versions.clone(),
                )
                .await;
                generator.run(stop_receiver.clone(), opt.batch_size)
            }
        };

        tasks.push(tokio::spawn(prometheus_task));
        tasks.push(tokio::spawn(witness_generator_task));

        tracing::info!(
            "initialized {:?} witness generator in {:?}",
            round,
            started_at.elapsed()
        );
        SERVER_METRICS.init_latency[&(*round).into()].set(started_at.elapsed());
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
