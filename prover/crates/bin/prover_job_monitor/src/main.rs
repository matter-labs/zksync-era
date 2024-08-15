use std::time::Duration;

use anyhow::Context as _;
use clap::Parser;
use tokio::{
    sync::{oneshot, watch},
    task::JoinHandle,
};
use zksync_config::configs::{
    fri_prover_group::FriProverGroupConfig, FriProofCompressorConfig, FriProverConfig,
    FriWitnessGeneratorConfig, ProverJobMonitorConfig,
};
use zksync_core_leftovers::temp_config_store::{load_database_secrets, load_general_config};
use zksync_periodic_job::PeriodicJob;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_job_monitor::{
    archiver::{GpuProverArchiver, ProverJobsArchiver},
    job_requeuer::{ProofCompressorJobRequeuer, ProverJobRequeuer, WitnessGeneratorJobRequeuer},
    queue_reporter::{
        ProofCompressorQueueReporter, ProverQueueReporter, WitnessGeneratorQueueReporter,
    },
    witness_job_queuer::WitnessJobQueuer,
};
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
    let proof_compressor_config = general_config
        .proof_compressor_config
        .context("proof_compressor_config")?;
    let prover_config = general_config.prover_config.context("prover_config")?;
    let witness_generator_config = general_config
        .witness_generator_config
        .context("witness_generator_config")?;
    let prover_group_config = general_config
        .prover_group_config
        .context("fri_prover_group_config")?;
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

    let connection_pool = ConnectionPool::<Prover>::builder(
        database_secrets.prover_url()?,
        prover_job_monitoring_config.max_db_connections,
    )
    .build()
    .await
    .context("failed to build a connection pool")?;

    let graceful_shutdown_timeout_ms =
        prover_job_monitoring_config.graceful_shutdown_timeout_ms as u64;

    let mut tasks = vec![tokio::spawn(exporter_config.run(stop_receiver.clone()))];

    tasks.extend(get_tasks(
        connection_pool,
        prover_job_monitoring_config,
        proof_compressor_config,
        prover_config,
        witness_generator_config,
        prover_group_config,
        stop_receiver,
    )?);
    let mut tasks = ManagedTasks::new(tasks);

    tokio::select! {
        _ = tasks.wait_single() => {},
        _ = stop_signal_receiver => {
            tracing::info!("Stop signal received, shutting down");
        }
    }
    stop_sender.send(true).ok();
    tasks
        .complete(Duration::from_millis(graceful_shutdown_timeout_ms))
        .await;

    Ok(())
}

fn get_tasks(
    connection_pool: ConnectionPool<Prover>,
    prover_job_monitor_config: ProverJobMonitorConfig,
    proof_compressor_config: FriProofCompressorConfig,
    prover_config: FriProverConfig,
    witness_generator_config: FriWitnessGeneratorConfig,
    prover_group_config: FriProverGroupConfig,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<Vec<JoinHandle<anyhow::Result<()>>>> {
    // archivers
    let gpu_prover_archiver = GpuProverArchiver::new(
        connection_pool.clone(),
        prover_job_monitor_config.gpu_prover_archiver_run_interval_ms,
        prover_job_monitor_config.gpu_prover_archiver_archive_prover_after_secs,
    );
    let prover_jobs_archiver = ProverJobsArchiver::new(
        connection_pool.clone(),
        prover_job_monitor_config.prover_jobs_archiver_run_interval_ms,
        prover_job_monitor_config.prover_jobs_archiver_archive_jobs_after_secs,
    );

    // job requeuers
    let proof_compressor_job_requeuer = ProofCompressorJobRequeuer::new(
        connection_pool.clone(),
        proof_compressor_config.max_attempts,
        proof_compressor_config.generation_timeout(),
        prover_job_monitor_config.proof_compressor_job_requeuer_run_interval_ms,
    );
    let prover_job_requeuer = ProverJobRequeuer::new(
        connection_pool.clone(),
        prover_config.max_attempts,
        prover_config.proof_generation_timeout(),
        prover_job_monitor_config.prover_job_requeuer_run_interval_ms,
    );
    let witness_generator_job_requeuer = WitnessGeneratorJobRequeuer::new(
        connection_pool.clone(),
        witness_generator_config.max_attempts,
        witness_generator_config.witness_generation_timeouts(),
        prover_job_monitor_config.witness_generator_job_requeuer_run_interval_ms,
    );

    // queue reporters
    let proof_compressor_queue_reporter = ProofCompressorQueueReporter::new(
        connection_pool.clone(),
        prover_job_monitor_config.proof_compressor_queue_reporter_run_interval_ms,
    );
    let prover_queue_reporter = ProverQueueReporter::new(
        connection_pool.clone(),
        prover_group_config,
        prover_job_monitor_config.prover_queue_reporter_run_interval_ms,
    );
    let witness_generator_queue_reporter = WitnessGeneratorQueueReporter::new(
        connection_pool.clone(),
        prover_job_monitor_config.witness_generator_queue_reporter_run_interval_ms,
    );

    // witness job queuer
    let witness_job_queuer = WitnessJobQueuer::new(
        connection_pool.clone(),
        prover_job_monitor_config.witness_job_queuer_run_interval_ms,
    );

    Ok(vec![
        tokio::spawn(gpu_prover_archiver.run(stop_receiver.clone())),
        tokio::spawn(prover_jobs_archiver.run(stop_receiver.clone())),
        tokio::spawn(proof_compressor_job_requeuer.run(stop_receiver.clone())),
        tokio::spawn(prover_job_requeuer.run(stop_receiver.clone())),
        tokio::spawn(witness_generator_job_requeuer.run(stop_receiver.clone())),
        tokio::spawn(proof_compressor_queue_reporter.run(stop_receiver.clone())),
        tokio::spawn(prover_queue_reporter.run(stop_receiver.clone())),
        tokio::spawn(witness_generator_queue_reporter.run(stop_receiver.clone())),
        tokio::spawn(witness_job_queuer.run(stop_receiver.clone())),
    ])
}
