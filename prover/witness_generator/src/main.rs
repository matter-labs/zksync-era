#![feature(generic_const_exprs)]

use anyhow::Context as _;
use prometheus_exporter::PrometheusExporterConfig;
use std::time::Instant;
use structopt::StructOpt;
use tokio::sync::watch;
use zksync_config::configs::{FriWitnessGeneratorConfig, PrometheusConfig};
use zksync_config::{FromEnv, ObjectStoreConfig};
use zksync_dal::{connection::DbVariant, ConnectionPool};
use zksync_object_store::ObjectStoreFactory;
use zksync_prover_utils::get_stop_signal_receiver;
use zksync_queued_job_processor::JobProcessor;
use zksync_types::proofs::AggregationRound;
use zksync_types::web3::futures::StreamExt;
use zksync_utils::wait_for_tasks::wait_for_tasks;
use zksync_vk_setup_data_server_fri::commitment_utils::get_cached_commitments;

use crate::basic_circuits::BasicWitnessGenerator;
use crate::leaf_aggregation::LeafAggregationWitnessGenerator;
use crate::node_aggregation::NodeAggregationWitnessGenerator;
use crate::scheduler::SchedulerWitnessGenerator;

mod basic_circuits;
mod leaf_aggregation;
mod node_aggregation;
mod precalculated_merkle_paths_provider;
mod scheduler;
mod utils;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "Run witness generator for different aggregation round",
    about = "Component for generating witness"
)]
struct Opt {
    /// Number of times witness generator should be run.
    #[structopt(short = "b", long = "batch_size")]
    batch_size: Option<usize>,
    /// aggregation round for the witness generator.
    #[structopt(short = "r", long = "round")]
    round: AggregationRound,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let log_format = vlog::log_format_from_env();
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let sentry_url = vlog::sentry_url_from_env();
    #[allow(deprecated)] // TODO (QIT-21): Use centralized configuration approach.
    let environment = vlog::environment_from_env();

    let mut builder = vlog::ObservabilityBuilder::new().with_log_format(log_format);
    if let Some(sentry_url) = &sentry_url {
        builder = builder
            .with_sentry_url(sentry_url)
            .context("Invalid Sentry URL")?
            .with_sentry_environment(environment);
    }
    let _guard = builder.build();

    // Report whether sentry is running after the logging subsystem was initialized.
    if let Some(sentry_url) = sentry_url {
        tracing::info!("Sentry configured with URL: {sentry_url}",);
    } else {
        tracing::info!("No sentry URL was provided");
    }

    let opt = Opt::from_args();
    let started_at = Instant::now();
    let use_push_gateway = opt.batch_size.is_some();

    let store_factory =
        ObjectStoreFactory::prover_from_env().context("ObjectStoreFactor::prover_from_env()")?;
    let config =
        FriWitnessGeneratorConfig::from_env().context("FriWitnessGeneratorConfig::from_env()")?;
    let prometheus_config = PrometheusConfig::from_env().context("PrometheusConfig::from_env()")?;
    let connection_pool = ConnectionPool::builder(DbVariant::Master)
        .build()
        .await
        .context("failed to build a connection_pool")?;
    let prover_connection_pool = ConnectionPool::builder(DbVariant::Prover)
        .build()
        .await
        .context("failed to build a prover_connection_pool")?;
    let (stop_sender, stop_receiver) = watch::channel(false);
    let vk_commitments = get_cached_commitments();
    let protocol_versions = prover_connection_pool
        .access_storage()
        .await
        .unwrap()
        .fri_protocol_versions_dal()
        .protocol_version_for(&vk_commitments)
        .await;

    tracing::info!(
        "initializing the {:?} witness generator, batch size: {:?} with protocol_versions: {:?}",
        opt.round,
        opt.batch_size,
        protocol_versions
    );

    // If batch_size is none, it means that the job is 'looping forever' (this is the usual setup in local network).
    // At the same time, we're reading the protocol_version only once at startup - so if there is no protocol version
    // read (this is often due to the fact, that the gateway was started too late, and it didn't put the updated protocol
    // versions into the database) - then the job will simply 'hang forever' and not pick any tasks.
    if opt.batch_size.is_none() && protocol_versions.is_empty() {
        panic!(
            "Could not find a protocol version for my commitments. Is gateway running?  Maybe you started this job before gateway updated the database? Commitments: {:?}",
            vk_commitments
        );
    }

    let prometheus_config = if use_push_gateway {
        PrometheusExporterConfig::push(
            prometheus_config.gateway_endpoint(),
            prometheus_config.push_interval(),
        )
    } else {
        PrometheusExporterConfig::pull(prometheus_config.listener_port)
    };
    let prometheus_task = prometheus_config.run(stop_receiver.clone());

    let witness_generator_task = match opt.round {
        AggregationRound::BasicCircuits => {
            let public_blob_store = match config.shall_save_to_public_bucket {
                false => None,
                true => Some(
                    ObjectStoreFactory::new(
                        ObjectStoreConfig::public_from_env()
                            .context("ObjectStoreConfig::public_from_env()")?,
                    )
                    .create_store()
                    .await,
                ),
            };
            let generator = BasicWitnessGenerator::new(
                config,
                &store_factory,
                public_blob_store,
                connection_pool,
                prover_connection_pool,
                protocol_versions.clone(),
            )
            .await;
            generator.run(stop_receiver, opt.batch_size)
        }
        AggregationRound::LeafAggregation => {
            let generator = LeafAggregationWitnessGenerator::new(
                config,
                &store_factory,
                prover_connection_pool,
                protocol_versions.clone(),
            )
            .await;
            generator.run(stop_receiver, opt.batch_size)
        }
        AggregationRound::NodeAggregation => {
            let generator = NodeAggregationWitnessGenerator::new(
                &store_factory,
                prover_connection_pool,
                protocol_versions.clone(),
            )
            .await;
            generator.run(stop_receiver, opt.batch_size)
        }
        AggregationRound::Scheduler => {
            let generator = SchedulerWitnessGenerator::new(
                &store_factory,
                prover_connection_pool,
                protocol_versions,
            )
            .await;
            generator.run(stop_receiver, opt.batch_size)
        }
    };

    let tasks = vec![
        tokio::spawn(prometheus_task),
        tokio::spawn(witness_generator_task),
    ];
    tracing::info!(
        "initialized {:?} witness generator in {:?}",
        opt.round,
        started_at.elapsed()
    );
    metrics::gauge!(
        "server.init.latency",
        started_at.elapsed(),
        "stage" => format!("fri_witness_generator_{:?}", opt.round)
    );

    let mut stop_signal_receiver = get_stop_signal_receiver();
    let graceful_shutdown = None::<futures::future::Ready<()>>;
    let tasks_allowed_to_finish = true;
    tokio::select! {
        _ = wait_for_tasks(tasks, None, graceful_shutdown, tasks_allowed_to_finish) => {},
        _ = stop_signal_receiver.next() => {
            tracing::info!("Stop signal received, shutting down");
        }
    }

    stop_sender.send(true).ok();
    tracing::info!("Finished witness generation");
    Ok(())
}
