#![feature(generic_const_exprs)]

use prometheus_exporter::run_prometheus_exporter;
use std::time::Instant;
use structopt::StructOpt;
use tokio::sync::watch;
use zksync_config::configs::{AlertsConfig, FriWitnessGeneratorConfig, PrometheusConfig};
use zksync_config::ObjectStoreConfig;
use zksync_dal::{connection::DbVariant, ConnectionPool};
use zksync_object_store::ObjectStoreFactory;
use zksync_prover_utils::get_stop_signal_receiver;
use zksync_queued_job_processor::JobProcessor;
use zksync_types::proofs::AggregationRound;
use zksync_types::web3::futures::StreamExt;
use zksync_utils::wait_for_tasks::wait_for_tasks;

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
async fn main() {
    vlog::init();
    let sentry_guard = vlog::init_sentry();
    match sentry_guard {
        Some(_) => vlog::info!(
            "Starting Sentry url: {}",
            std::env::var("MISC_SENTRY_URL").unwrap(),
        ),
        None => vlog::info!("No sentry url configured"),
    }

    let opt = Opt::from_args();
    let started_at = Instant::now();
    vlog::info!(
        "initializing the {:?} witness generator, batch size: {:?}",
        opt.round,
        opt.batch_size
    );
    let use_push_gateway = opt.batch_size.is_some();

    let store_factory = ObjectStoreFactory::from_env();
    let config = FriWitnessGeneratorConfig::from_env();
    let prometheus_config = PrometheusConfig::from_env();
    let connection_pool = ConnectionPool::builder(DbVariant::Master).build().await;
    let prover_connection_pool = ConnectionPool::builder(DbVariant::Prover).build().await;
    let (stop_sender, stop_receiver) = watch::channel(false);

    let witness_generator_task = match opt.round {
        AggregationRound::BasicCircuits => {
            let public_blob_store = ObjectStoreFactory::new(ObjectStoreConfig::public_from_env())
                .create_store()
                .await;
            let generator = BasicWitnessGenerator::new(
                config,
                &store_factory,
                public_blob_store,
                connection_pool,
                prover_connection_pool,
            )
            .await;
            generator.run(stop_receiver, opt.batch_size)
        }
        AggregationRound::LeafAggregation => {
            let generator = LeafAggregationWitnessGenerator::new(
                config,
                &store_factory,
                prover_connection_pool,
            )
            .await;
            generator.run(stop_receiver, opt.batch_size)
        }
        AggregationRound::NodeAggregation => {
            let generator =
                NodeAggregationWitnessGenerator::new(&store_factory, prover_connection_pool).await;
            generator.run(stop_receiver, opt.batch_size)
        }
        AggregationRound::Scheduler => {
            let generator =
                SchedulerWitnessGenerator::new(&store_factory, prover_connection_pool).await;
            generator.run(stop_receiver, opt.batch_size)
        }
    };
    let tasks = vec![
        run_prometheus_exporter(
            prometheus_config.listener_port,
            use_push_gateway.then(|| {
                (
                    prometheus_config.pushgateway_url.clone(),
                    prometheus_config.push_interval(),
                )
            }),
        ),
        tokio::spawn(witness_generator_task),
    ];
    vlog::info!(
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
    let particular_crypto_alerts = Some(AlertsConfig::from_env().sporadic_crypto_errors_substrs);
    let graceful_shutdown = None::<futures::future::Ready<()>>;
    let tasks_allowed_to_finish = false;
    tokio::select! {
        _ = wait_for_tasks(tasks, particular_crypto_alerts, graceful_shutdown, tasks_allowed_to_finish) => {},
        _ = stop_signal_receiver.next() => {
            vlog::info!("Stop signal received, shutting down");
        }
    }

    stop_sender.send(true).ok();
    vlog::info!("Finished witness generation");
}
