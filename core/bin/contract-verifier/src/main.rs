use std::{path::PathBuf, time::Duration};

use anyhow::Context as _;
use clap::Parser;
use tokio::sync::watch;
use zksync_config::{
    configs::{DatabaseSecrets, ObservabilityConfig, PrometheusConfig},
    full_config_schema,
    sources::ConfigFilePaths,
    ConfigRepository, ContractVerifierConfig, ParseResultExt,
};
use zksync_contract_verifier_lib::ContractVerifier;
use zksync_dal::{ConnectionPool, Core};
use zksync_queued_job_processor::JobProcessor;
use zksync_utils::wait_for_tasks::ManagedTasks;
use zksync_vlog::prometheus::PrometheusExporterConfig;

#[derive(Debug, Parser)]
#[command(name = "ZKsync contract code verifier", author = "Matter Labs")]
struct Opt {
    /// Number of jobs to process. If None, runs indefinitely.
    #[arg(long)]
    jobs_number: Option<usize>,
    /// Path to the configuration file.
    #[arg(long)]
    config_path: Option<PathBuf>,
    /// Path to the secrets file.
    #[arg(long)]
    secrets_path: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();

    let config_file_paths = ConfigFilePaths {
        general: opt.config_path,
        secrets: opt.secrets_path,
        ..ConfigFilePaths::default()
    };
    let config_sources =
        tokio::task::spawn_blocking(|| config_file_paths.into_config_sources("")).await??;

    let observability_config =
        ObservabilityConfig::from_sources(config_sources.clone()).context("ObservabilityConfig")?;
    let _observability_guard = observability_config.install()?;

    let schema = full_config_schema(false);
    let repo = ConfigRepository::new(&schema).with_all(config_sources);
    let database_secrets: DatabaseSecrets = repo.single()?.parse().log_all_errors()?;
    let verifier_config: ContractVerifierConfig = repo.single()?.parse().log_all_errors()?;
    let mut prometheus_config: PrometheusConfig = repo.single()?.parse().log_all_errors()?;
    prometheus_config.listener_port = verifier_config.prometheus_port;

    let pool = ConnectionPool::<Core>::singleton(
        database_secrets
            .master_url()
            .context("Master DB URL is absent")?,
    )
    .build()
    .await?;

    let (stop_sender, stop_receiver) = watch::channel(false);
    let contract_verifier = ContractVerifier::new(verifier_config.compilation_timeout, pool)
        .await
        .context("failed initializing contract verifier")?;
    let update_task = contract_verifier.sync_compiler_versions_task();
    let tasks = vec![
        tokio::spawn(update_task),
        tokio::spawn(contract_verifier.run(stop_receiver.clone(), opt.jobs_number)),
        tokio::spawn(
            PrometheusExporterConfig::pull(prometheus_config.listener_port).run(stop_receiver),
        ),
    ];

    let mut tasks = ManagedTasks::new(tasks);
    tokio::select! {
        () = tasks.wait_single() => {},
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Stop signal received, shutting down");
        },
    };
    stop_sender.send_replace(true);

    // Sleep for some time to let verifier gracefully stop.
    tasks.complete(Duration::from_secs(5)).await;
    Ok(())
}
