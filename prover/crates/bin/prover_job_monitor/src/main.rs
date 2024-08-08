use std::time::Duration;

use anyhow::Context as _;
use clap::Parser;
use tokio::sync::{oneshot, watch};
use zksync_core_leftovers::temp_config_store::{load_database_secrets, load_general_config};
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_utils::wait_for_tasks::ManagedTasks;
use zksync_vlog::prometheus::PrometheusExporterConfig;

#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version)]
pub(crate) struct CliOpts {
    #[arg(long)]
    pub(crate) config_path: Option<std::path::PathBuf>,
    #[arg(long)]
    pub(crate) secrets_path: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = CliOpts::parse();

    let general_config = load_general_config(opt.config_path).context("general config")?;

    let database_secrets = load_database_secrets(opt.secrets_path).context("database secrets")?;

    let observability_config = general_config
        .observability
        .context("observability config")?;
    let _observability_guard = observability_config.install()?;

    let prover_job_monitoring_config = general_config
        .prover_job_monitor_config
        .context("prover_job_monitoring_config")?;
    let exporter_config =
        PrometheusExporterConfig::pull(prover_job_monitoring_config.prometheus_port);

    let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
    let mut stop_signal_sender = Some(stop_signal_sender);
    ctrlc::set_handler(move || {
        if let Some(sender) = stop_signal_sender.take() {
            sender.send(()).ok();
        }
    })
    .context("Error setting Ctrl+C handler")?;

    let (stop_sender, stop_receiver) = watch::channel(false);

    tracing::info!("Starting ProverJobMonitoring");

    let prover_connection_pool = ConnectionPool::<Prover>::builder(
        database_secrets.prover_url()?,
        prover_job_monitoring_config.max_db_connections,
    )
    .build()
    .await
    .context("failed to build a connection pool")?;

    let tasks = vec![
        tokio::spawn(async {
            println!("hello world");
            Ok(())
        }), // tokio::spawn(
            //     PrometheusExporterConfig::pull(config.prometheus_listener_port)
            //         .run(stop_receiver.clone()),
            // ),
            // tokio::spawn(proof_gen_data_fetcher.run(config.api_poll_duration(), stop_receiver.clone())),
            // tokio::spawn(proof_submitter.run(config.api_poll_duration(), stop_receiver)),
    ];

    let mut tasks = ManagedTasks::new(tasks);

    tokio::select! {
        _ = tasks.wait_single() => {},
        _ = stop_signal_receiver => {
            tracing::info!("Stop signal received, shutting down");
        }
    }
    stop_sender.send(true).ok();
    tasks
        .complete(Duration::from_millis(
            prover_job_monitoring_config
                .graceful_shutdown_timeout_ms
                .into(),
        ))
        .await;

    Ok(())
}

// pub fn stuff() {
//     let prover_config = general_config.prover_config.context("fri_prover config")?;
//     let exporter_config = PrometheusExporterConfig::pull(prover_config.prometheus_port);
//
//     let (stop_signal_sender, stop_signal_receiver) = oneshot::channel();
//     let mut stop_signal_sender = Some(stop_signal_sender);
//     ctrlc::set_handler(move || {
//         if let Some(sender) = stop_signal_sender.take() {
//             sender.send(()).ok();
//         }
//     })
//         .context("Error setting Ctrl+C handler")?;
//
//     let zone = RegionFetcher::new(
//         prover_config.cloud_type,
//         prover_config.zone_read_url.clone(),
//     )
//         .get_zone()
//         .await?;
//
//     let (stop_sender, stop_receiver) = tokio::sync::watch::channel(false);
//     let prover_object_store_config = prover_config
//         .prover_object_store
//         .clone()
//         .context("prover object store config")?;
//     let object_store_factory = ObjectStoreFactory::new(prover_object_store_config);
//     let public_object_store_config = prover_config
//         .public_object_store
//         .clone()
//         .context("public object store config")?;
//     let public_blob_store = match prover_config.shall_save_to_public_bucket {
//         false => None,
//         true => Some(
//             ObjectStoreFactory::new(public_object_store_config)
//                 .create_store()
//                 .await?,
//         ),
//     };
//     let specialized_group_id = prover_config.specialized_group_id;
//
//     let circuit_ids_for_round_to_be_proven = general_config
//         .prover_group_config
//         .context("prover group config")?
//         .get_circuit_ids_for_group_id(specialized_group_id)
//         .unwrap_or_default();
//     let circuit_ids_for_round_to_be_proven =
//         get_all_circuit_id_round_tuples_for(circuit_ids_for_round_to_be_proven);
//
//     tracing::info!(
// "Starting FRI proof generation for group: {} with circuits: {:?}",
// specialized_group_id,
// circuit_ids_for_round_to_be_proven.clone()
// );
//
//     // There are 2 threads using the connection pool:
//     // 1. The prover thread, which is used to update the prover job status.
//     // 2. The socket listener thread, which is used to update the prover instance status.
//     const MAX_POOL_SIZE_FOR_PROVER: u32 = 2;
//
//     let pool = ConnectionPool::builder(database_secrets.prover_url()?, MAX_POOL_SIZE_FOR_PROVER)
//         .build()
//         .await
//         .context("failed to build a connection pool")?;
//     let port = prover_config.witness_vector_receiver_port;
//
//     let notify = Arc::new(Notify::new());
//
//     let prover_tasks = get_prover_tasks(
//         prover_config,
//         zone.clone(),
//         stop_receiver.clone(),
//         object_store_factory,
//         public_blob_store,
//         pool,
//         circuit_ids_for_round_to_be_proven,
//         notify,
//     )
//         .await
//         .context("get_prover_tasks()")?;
//
//     let mut tasks = vec![tokio::spawn(exporter_config.run(stop_receiver))];
//
//     tasks.extend(prover_tasks);
//
//     let mut tasks = ManagedTasks::new(tasks);
//     tokio::select! {
// _ = tasks.wait_single() => {
// if cfg!(feature = "gpu") {
// graceful_shutdown(zone, port)
// .await
// .context("failed to prepare graceful shutdown future")?
// .await;
// }
// },
// _ = stop_signal_receiver => {
// tracing::info!("Stop signal received, shutting down");
// },
// }
//
//     stop_sender.send(true).ok();
//     tasks.complete(Duration::from_secs(5)).await;
//     Ok(())
// }

// #![allow(incomplete_features)] // We have to use generic const exprs.
// #![feature(generic_const_exprs)]
//
// use std::time::{Duration, Instant};
//
// use anyhow::{anyhow, Context as _};
// use futures::{channel::mpsc, executor::block_on, SinkExt, StreamExt};
// use structopt::StructOpt;
// use tokio::sync::watch;
// use zksync_core_leftovers::temp_config_store::{load_database_secrets, load_general_config};
// use zksync_env_config::object_store::ProverObjectStoreConfig;
// use zksync_object_store::ObjectStoreFactory;
// use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
// use zksync_prover_fri_types::PROVER_PROTOCOL_SEMANTIC_VERSION;
// use zksync_queued_job_processor::JobProcessor;
// use zksync_types::basic_fri_types::AggregationRound;
// use zksync_utils::wait_for_tasks::ManagedTasks;
// use zksync_vk_setup_data_server_fri::commitment_utils::get_cached_commitments;
// use zksync_vlog::prometheus::PrometheusExporterConfig;
//
// use crate::{
//     basic_circuits::BasicWitnessGenerator, leaf_aggregation::LeafAggregationWitnessGenerator,
//     metrics::SERVER_METRICS, node_aggregation::NodeAggregationWitnessGenerator,
//     recursion_tip::RecursionTipWitnessGenerator, scheduler::SchedulerWitnessGenerator,
// };
//
// mod basic_circuits;
// mod leaf_aggregation;
// mod metrics;
// mod node_aggregation;
// mod precalculated_merkle_paths_provider;
// mod recursion_tip;
// mod scheduler;
// mod storage_oracle;
// mod utils;
//
// #[cfg(not(target_env = "msvc"))]
// use jemallocator::Jemalloc;
//
// #[cfg(not(target_env = "msvc"))]
// #[global_allocator]
// static GLOBAL: Jemalloc = Jemalloc;
//
// #[derive(Debug, StructOpt)]
// #[structopt(
//     name = "Run witness generator for different aggregation round",
//     about = "Component for generating witness"
// )]
// struct Opt {
//     /// Number of times witness generator should be run.
//     #[structopt(short = "b", long = "batch_size")]
//     batch_size: Option<usize>,
//     /// Aggregation rounds options, they can be run individually or together.
//     ///
//     /// Single aggregation round for the witness generator.
//     #[structopt(short = "r", long = "round")]
//     round: Option<AggregationRound>,
//     /// Start all aggregation rounds for the witness generator.
//     #[structopt(short = "a", long = "all_rounds")]
//     all_rounds: bool,
//     /// Path to the configuration file.
//     #[structopt(long)]
//     config_path: Option<std::path::PathBuf>,
//     /// Path to the secrets file.
//     #[structopt(long)]
//     secrets_path: Option<std::path::PathBuf>,
// }
//
// #[tokio::main]
// async fn main() -> anyhow::Result<()> {
//     let opt = Opt::from_args();
//
//     let general_config = load_general_config(opt.config_path).context("general config")?;
//
//     let database_secrets = load_database_secrets(opt.secrets_path).context("database secrets")?;
//
//     let observability_config = general_config
//         .observability
//         .context("observability config")?;
//     let _observability_guard = observability_config.install()?;
//
//     let started_at = Instant::now();
//     let use_push_gateway = opt.batch_size.is_some();
//
//     let prover_config = general_config.prover_config.context("prover config")?;
//     let object_store_config = ProverObjectStoreConfig(
//         prover_config
//             .prover_object_store
//             .context("object store")?
//             .clone(),
//     );
//     let store_factory = ObjectStoreFactory::new(object_store_config.0);
//     let config = general_config
//         .witness_generator
//         .context("witness generator config")?;
//
//     let prometheus_config = general_config.prometheus_config;
//
//     // If the prometheus listener port is not set in the witness generator config, use the one from the prometheus config.
//     let prometheus_listener_port = if let Some(port) = config.prometheus_listener_port {
//         port
//     } else {
//         prometheus_config
//             .clone()
//             .context("prometheus config")?
//             .listener_port
//     };
//
//     let prover_connection_pool =
//         ConnectionPool::<Prover>::singleton(database_secrets.prover_url()?)
//             .build()
//             .await
//             .context("failed to build a prover_connection_pool")?;
//     let (stop_sender, stop_receiver) = watch::channel(false);
//
//     let protocol_version = PROVER_PROTOCOL_SEMANTIC_VERSION;
//     let vk_commitments_in_db = match prover_connection_pool
//         .connection()
//         .await
//         .unwrap()
//         .fri_protocol_versions_dal()
//         .vk_commitments_for(protocol_version)
//         .await
//     {
//         Some(commitments) => commitments,
//         None => {
//             panic!(
//                 "No vk commitments available in database for a protocol version {:?}.",
//                 protocol_version
//             );
//         }
//     };
//
//     let rounds = match (opt.round, opt.all_rounds) {
//         (Some(round), false) => vec![round],
//         (None, true) => vec![
//             AggregationRound::BasicCircuits,
//             AggregationRound::LeafAggregation,
//             AggregationRound::NodeAggregation,
//             AggregationRound::RecursionTip,
//             AggregationRound::Scheduler,
//         ],
//         (Some(_), true) => {
//             return Err(anyhow!(
//                 "Cannot set both the --all_rounds and --round flags. Choose one or the other."
//             ));
//         }
//         (None, false) => {
//             return Err(anyhow!(
//                 "Expected --all_rounds flag with no --round flag present"
//             ));
//         }
//     };
//
//     let prometheus_config = if use_push_gateway {
//         let prometheus_config = prometheus_config
//             .clone()
//             .context("prometheus config needed when use_push_gateway enabled")?;
//         PrometheusExporterConfig::push(
//             prometheus_config
//                 .gateway_endpoint()
//                 .context("gateway_endpoint needed when use_push_gateway enabled")?,
//             prometheus_config.push_interval(),
//         )
//     } else {
//         PrometheusExporterConfig::pull(prometheus_listener_port as u16)
//     };
//     let prometheus_task = prometheus_config.run(stop_receiver.clone());
//
//     let mut tasks = Vec::new();
//     tasks.push(tokio::spawn(prometheus_task));
//
//     for round in rounds {
//         tracing::info!(
//             "initializing the {:?} witness generator, batch size: {:?} with protocol_version: {:?}",
//             round,
//             opt.batch_size,
//             &protocol_version
//         );
//
//         let witness_generator_task = match round {
//             AggregationRound::BasicCircuits => {
//                 let setup_data_path = prover_config.setup_data_path.clone();
//                 let vk_commitments = get_cached_commitments(Some(setup_data_path));
//                 assert_eq!(
//                     vk_commitments,
//                     vk_commitments_in_db,
//                     "VK commitments didn't match commitments from DB for protocol version {protocol_version:?}. Cached commitments: {vk_commitments:?}, commitments in database: {vk_commitments_in_db:?}"
//                 );
//
//                 let public_blob_store = match config.shall_save_to_public_bucket {
//                     false => None,
//                     true => Some(
//                         ObjectStoreFactory::new(
//                             prover_config
//                                 .public_object_store
//                                 .clone()
//                                 .expect("public_object_store"),
//                         )
//                             .create_store()
//                             .await?,
//                     ),
//                 };
//                 let generator = BasicWitnessGenerator::new(
//                     config.clone(),
//                     store_factory.create_store().await?,
//                     public_blob_store,
//                     prover_connection_pool.clone(),
//                     protocol_version,
//                 );
//                 generator.run(stop_receiver.clone(), opt.batch_size)
//             }
//             AggregationRound::LeafAggregation => {
//                 let generator = LeafAggregationWitnessGenerator::new(
//                     config.clone(),
//                     store_factory.create_store().await?,
//                     prover_connection_pool.clone(),
//                     protocol_version,
//                 );
//                 generator.run(stop_receiver.clone(), opt.batch_size)
//             }
//             AggregationRound::NodeAggregation => {
//                 let generator = NodeAggregationWitnessGenerator::new(
//                     config.clone(),
//                     store_factory.create_store().await?,
//                     prover_connection_pool.clone(),
//                     protocol_version,
//                 );
//                 generator.run(stop_receiver.clone(), opt.batch_size)
//             }
//             AggregationRound::RecursionTip => {
//                 let generator = RecursionTipWitnessGenerator::new(
//                     config.clone(),
//                     store_factory.create_store().await?,
//                     prover_connection_pool.clone(),
//                     protocol_version,
//                 );
//                 generator.run(stop_receiver.clone(), opt.batch_size)
//             }
//             AggregationRound::Scheduler => {
//                 let generator = SchedulerWitnessGenerator::new(
//                     config.clone(),
//                     store_factory.create_store().await?,
//                     prover_connection_pool.clone(),
//                     protocol_version,
//                 );
//                 generator.run(stop_receiver.clone(), opt.batch_size)
//             }
//         };
//
//         tasks.push(tokio::spawn(witness_generator_task));
//
//         tracing::info!(
//             "initialized {:?} witness generator in {:?}",
//             round,
//             started_at.elapsed()
//         );
//         SERVER_METRICS.init_latency[&round.into()].set(started_at.elapsed());
//     }
//
//     let (mut stop_signal_sender, mut stop_signal_receiver) = mpsc::channel(256);
//     ctrlc::set_handler(move || {
//         block_on(stop_signal_sender.send(true)).expect("Ctrl+C signal send");
//     })
//         .expect("Error setting Ctrl+C handler");
//     let mut tasks = ManagedTasks::new(tasks).allow_tasks_to_finish();
//     tokio::select! {
//         _ = tasks.wait_single() => {},
//         _ = stop_signal_receiver.next() => {
//             tracing::info!("Stop signal received, shutting down");
//         }
//     }
//
//     stop_sender.send_replace(true);
//     tasks.complete(Duration::from_secs(5)).await;
//     tracing::info!("Finished witness generation");
//     Ok(())
// }
