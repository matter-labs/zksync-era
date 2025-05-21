use std::{future::IntoFuture, net::SocketAddr, time::Duration};

use anyhow::Context as _;
use clap::Parser;
use tokio::{
    sync::{oneshot, watch},
    task::JoinHandle,
};
use zksync_config::{
    configs::{
        DatabaseSecrets, FriProofCompressorConfig, FriProverConfig, FriWitnessGeneratorConfig,
        GeneralConfig, ProverJobMonitorConfig,
    },
    full_config_schema,
    sources::ConfigFilePaths,
    ConfigRepositoryExt,
};
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_job_monitor::{
    attempts_reporter::ProverJobAttemptsReporter,
    autoscaler_queue_reporter::get_queue_reporter_router,
    job_requeuer::{ProofCompressorJobRequeuer, ProverJobRequeuer, WitnessGeneratorJobRequeuer},
    prover_jobs_archiver::ProverJobsArchiver,
    queue_reporter::{
        ProofCompressorQueueReporter, ProverQueueReporter, WitnessGeneratorQueueReporter,
    },
    witness_job_queuer::WitnessJobQueuer,
};
use zksync_prover_task::TaskRunner;
use zksync_task_management::ManagedTasks;
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
    let schema = full_config_schema(false);
    let config_file_paths = ConfigFilePaths {
        general: opt.config_path,
        secrets: opt.secrets_path,
        ..ConfigFilePaths::default()
    };
    let config_sources = config_file_paths.into_config_sources("ZKSYNC_")?;

    let _observability_guard = config_sources.observability()?.install()?;

    let repo = config_sources.build_repository(&schema);
    let general_config: GeneralConfig = repo.parse()?;
    let database_secrets: DatabaseSecrets = repo.parse()?;

    let prover_job_monitor_config = general_config
        .prover_job_monitor_config
        .context("prover_job_monitor_config")?;
    let proof_compressor_config = general_config
        .proof_compressor_config
        .context("proof_compressor_config")?;
    let prover_config = general_config.prover_config.context("prover_config")?;
    let witness_generator_config = general_config
        .witness_generator_config
        .context("witness_generator_config")?;
    let exporter_config = PrometheusExporterConfig::pull(prover_job_monitor_config.prometheus_port);

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

    let connection_pool = ConnectionPool::<Prover>::builder(
        database_secrets.prover_url()?,
        prover_job_monitor_config.max_db_connections,
    )
    .build()
    .await
    .context("failed to build a connection pool")?;

    let graceful_shutdown_timeout = prover_job_monitor_config.graceful_shutdown_timeout;

    let mut tasks = vec![tokio::spawn(exporter_config.run(stop_receiver.clone()))];

    tasks.extend(get_tasks(
        connection_pool.clone(),
        prover_job_monitor_config.clone(),
        proof_compressor_config,
        prover_config,
        witness_generator_config,
        stop_receiver.clone(),
    )?);
    let mut tasks = ManagedTasks::new(tasks);

    let bind_address = SocketAddr::from(([0, 0, 0, 0], prover_job_monitor_config.http_port));

    tracing::info!("Starting PJM server on {bind_address}");

    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .with_context(|| format!("Failed binding PJM server to {bind_address}"))?;

    let mut receiver = stop_receiver.clone();
    let app = axum::serve(listener, get_queue_reporter_router(connection_pool))
        .with_graceful_shutdown(async move {
            if receiver.changed().await.is_err() {
                tracing::warn!(
                    "Stop request sender for PJM server was dropped without sending a signal"
                );
            }
            tracing::info!("Stop request received, PJM server is shutting down");
        })
        .into_future();

    tokio::select! {
        _ = tasks.wait_single() => {},
        _ = stop_signal_receiver => {
            tracing::info!("Stop request received, shutting down");
        }
        _ = app => {}
    }
    stop_sender.send(true).ok();
    tasks.complete(graceful_shutdown_timeout).await;

    Ok(())
}

fn get_tasks(
    connection_pool: ConnectionPool<Prover>,
    prover_job_monitor_config: ProverJobMonitorConfig,
    proof_compressor_config: FriProofCompressorConfig,
    prover_config: FriProverConfig,
    witness_generator_config: FriWitnessGeneratorConfig,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<Vec<JoinHandle<anyhow::Result<()>>>> {
    let mut task_runner = TaskRunner::default();

    // archivers
    let prover_jobs_archiver = ProverJobsArchiver::new(
        connection_pool.clone(),
        prover_job_monitor_config.prover_jobs_archiver_archive_jobs_after,
    );
    task_runner.add(
        "ProverJobsArchiver",
        prover_job_monitor_config.prover_jobs_archiver_run_interval,
        prover_jobs_archiver,
    );

    // job re-queuers
    let proof_compressor_job_requeuer = ProofCompressorJobRequeuer::new(
        connection_pool.clone(),
        proof_compressor_config.max_attempts,
        proof_compressor_config.generation_timeout_in_secs,
    );
    task_runner.add(
        "ProofCompressorJobRequeuer",
        prover_job_monitor_config.proof_compressor_job_requeuer_run_interval,
        proof_compressor_job_requeuer,
    );

    let prover_job_requeuer = ProverJobRequeuer::new(
        connection_pool.clone(),
        prover_config.max_attempts,
        prover_config.generation_timeout_in_secs,
    );
    task_runner.add(
        "ProverJobRequeuer",
        prover_job_monitor_config.prover_job_requeuer_run_interval,
        prover_job_requeuer,
    );

    let witness_generator_job_requeuer = WitnessGeneratorJobRequeuer::new(
        connection_pool.clone(),
        witness_generator_config.max_attempts,
        witness_generator_config.witness_generation_timeouts(),
    );
    task_runner.add(
        "WitnessGeneratorJobRequeuer",
        prover_job_monitor_config.witness_generator_job_requeuer_run_interval,
        witness_generator_job_requeuer,
    );

    // queue reporters
    let proof_compressor_queue_reporter =
        ProofCompressorQueueReporter::new(connection_pool.clone());
    task_runner.add(
        "ProofCompressorQueueReporter",
        prover_job_monitor_config.proof_compressor_queue_reporter_run_interval,
        proof_compressor_queue_reporter,
    );

    let prover_queue_reporter = ProverQueueReporter::new(connection_pool.clone());
    task_runner.add(
        "ProverQueueReporter",
        prover_job_monitor_config.prover_queue_reporter_run_interval,
        prover_queue_reporter,
    );

    let witness_generator_queue_reporter =
        WitnessGeneratorQueueReporter::new(connection_pool.clone());
    task_runner.add(
        "WitnessGeneratorQueueReporter",
        prover_job_monitor_config.witness_generator_queue_reporter_run_interval,
        witness_generator_queue_reporter,
    );

    // witness job queuer
    let witness_job_queuer = WitnessJobQueuer::new(connection_pool.clone());
    task_runner.add(
        "WitnessJobQueuer",
        prover_job_monitor_config.witness_job_queuer_run_interval,
        witness_job_queuer,
    );

    // Reporter for reaching max attempts of jobs
    let attempts_reporter = ProverJobAttemptsReporter::new(
        connection_pool,
        prover_config,
        witness_generator_config,
        proof_compressor_config,
    );
    task_runner.add(
        "ProverJobAttemptsReporter",
        Duration::from_secs(10),
        attempts_reporter,
    );

    Ok(task_runner.spawn(stop_receiver))
}
