use std::{path::PathBuf, time::Duration};

use anyhow::Context as _;
use clap::Parser;
use tokio::sync::watch;
use zksync_config::configs::PrometheusConfig;
use zksync_contract_verifier_lib::ContractVerifier;
use zksync_core_leftovers::temp_config_store::{load_database_secrets, load_general_config};
use zksync_dal::{ConnectionPool, Core, CoreDal};
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

async fn perform_storage_migration(pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
    const BATCH_SIZE: usize = 1000;

    // Make it possible to override just in case.
    let batch_size = std::env::var("CONTRACT_VERIFIER_MIGRATION_BATCH_SIZE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(BATCH_SIZE);

    let mut storage = pool.connection().await?;
    let migration_performed = storage
        .contract_verification_dal()
        .is_verification_info_migration_performed()
        .await?;
    if !migration_performed {
        tracing::info!(batch_size = %batch_size, "Running the storage migration for the contract verifier table");
        storage
            .contract_verification_dal()
            .perform_verification_info_migration(batch_size)
            .await?;
    } else {
        tracing::info!("Storage migration is not needed");
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();

    let general_config = load_general_config(opt.config_path).context("general config")?;
    let observability_config = general_config
        .observability
        .context("ObservabilityConfig")?;
    let _observability_guard = observability_config.install()?;

    let database_secrets = load_database_secrets(opt.secrets_path).context("database secrets")?;
    let verifier_config = general_config
        .contract_verifier
        .context("ContractVerifierConfig")?;
    let prometheus_config = PrometheusConfig {
        listener_port: verifier_config.prometheus_port,
        ..general_config.api_config.context("ApiConfig")?.prometheus
    };
    let pool = ConnectionPool::<Core>::singleton(
        database_secrets
            .master_url()
            .context("Master DB URL is absent")?,
    )
    .build()
    .await?;

    perform_storage_migration(&pool).await?;

    let (stop_sender, stop_receiver) = watch::channel(false);
    let contract_verifier = ContractVerifier::new(verifier_config.compilation_timeout(), pool)
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
