use std::time::Instant;

use futures::StreamExt;
use prometheus_exporter::run_prometheus_exporter;
use zksync_config::configs::WitnessGeneratorConfig;
use zksync_config::ZkSyncConfig;
use zksync_dal::ConnectionPool;
use zksync_object_store::ObjectStoreFactory;
use zksync_prover_utils::{get_stop_signal_receiver, wait_for_tasks};
use zksync_queued_job_processor::JobProcessor;
use zksync_types::proofs::AggregationRound;

use crate::basic_circuits::BasicWitnessGenerator;
use crate::leaf_aggregation::LeafAggregationWitnessGenerator;
use crate::node_aggregation::NodeAggregationWitnessGenerator;
use crate::scheduler::SchedulerWitnessGenerator;
use structopt::StructOpt;

mod basic_circuits;
mod leaf_aggregation;
mod node_aggregation;
mod precalculated;
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
    let opt = Opt::from_args();
    let _sentry_guard = vlog::init();
    let connection_pool = ConnectionPool::new(None, true);
    let zksync_config = ZkSyncConfig::from_env();
    let (stop_sender, stop_receiver) = tokio::sync::watch::channel::<bool>(false);
    let started_at = Instant::now();
    vlog::info!(
        "initializing the {:?} witness generator, batch size: {:?}",
        opt.round,
        opt.batch_size
    );
    let use_push_gateway = opt.batch_size.is_some();

    let config = WitnessGeneratorConfig::from_env();
    let store_factory = ObjectStoreFactory::from_env();
    let witness_generator_task = match opt.round {
        AggregationRound::BasicCircuits => {
            let generator = BasicWitnessGenerator::new(config, &store_factory);
            generator.run(connection_pool, stop_receiver, opt.batch_size)
        }
        AggregationRound::LeafAggregation => {
            let generator = LeafAggregationWitnessGenerator::new(config, &store_factory);
            generator.run(connection_pool, stop_receiver, opt.batch_size)
        }
        AggregationRound::NodeAggregation => {
            let generator = NodeAggregationWitnessGenerator::new(config, &store_factory);
            generator.run(connection_pool, stop_receiver, opt.batch_size)
        }
        AggregationRound::Scheduler => {
            let generator = SchedulerWitnessGenerator::new(config, &store_factory);
            generator.run(connection_pool, stop_receiver, opt.batch_size)
        }
    };

    let witness_generator_task = tokio::spawn(witness_generator_task);
    vlog::info!(
        "initialized {:?} witness generator in {:?}",
        opt.round,
        started_at.elapsed()
    );
    metrics::gauge!(
        "server.init.latency",
        started_at.elapsed(),
        "stage" => format!("witness_generator_{:?}", opt.round)
    );
    let tasks = vec![
        run_prometheus_exporter(zksync_config.api.prometheus, use_push_gateway),
        witness_generator_task,
    ];

    let mut stop_signal_receiver = get_stop_signal_receiver();
    tokio::select! {
        _ = wait_for_tasks(tasks) => {},
        _ = stop_signal_receiver.next() => {
            vlog::info!("Stop signal received, shutting down");
        },
    }
    let _ = stop_sender.send(true);
}
