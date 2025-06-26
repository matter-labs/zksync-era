use std::{path::PathBuf, time::Duration};

use anyhow::Context as _;
use clap::Parser;
use tokio::sync::watch;
use zksync_config::{
    configs::PostgresSecrets, full_config_schema, sources::ConfigFilePaths, ContractVerifierConfig,
};
use zksync_contract_verifier_lib::{
    etherscan::{metrics::EtherscanVerifierMetrics, EtherscanVerifier},
    ContractVerifier,
};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_queued_job_processor::JobProcessor;
use zksync_task_management::ManagedTasks;
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

    let config_file_paths = ConfigFilePaths {
        general: opt.config_path,
        secrets: opt.secrets_path,
        ..ConfigFilePaths::default()
    };
    let config_sources =
        tokio::task::spawn_blocking(|| config_file_paths.into_config_sources("ZKSYNC_")).await??;

    let _observability_guard = config_sources.observability()?.install()?;

    let schema = full_config_schema();
    let mut repo = config_sources.build_repository(&schema);
    let database_secrets: PostgresSecrets = repo.parse()?;
    let verifier_config: ContractVerifierConfig = repo.parse()?;

    let pool = ConnectionPool::<Core>::singleton(
        database_secrets
            .master_url()
            .context("Master DB URL is absent")?,
    )
    .build()
    .await?;

    perform_storage_migration(&pool).await?;

    let (stop_sender, stop_receiver) = watch::channel(false);
    let etherscan_api_key = verifier_config.etherscan_api_key;
    let etherscan_verifier_enabled =
        verifier_config.etherscan_api_url.is_some() && etherscan_api_key.is_some();
    let contract_verifier = ContractVerifier::new(
        verifier_config.compilation_timeout,
        pool.clone(),
        etherscan_verifier_enabled,
    )
    .await
    .context("failed initializing contract verifier")?;
    let update_task = contract_verifier.sync_compiler_versions_task();

    let mut tasks = vec![
        tokio::spawn(update_task),
        tokio::spawn(contract_verifier.run(stop_receiver.clone(), opt.jobs_number)),
        tokio::spawn(
            PrometheusExporterConfig::pull(verifier_config.prometheus_port)
                .run(stop_receiver.clone()),
        ),
    ];
    if etherscan_verifier_enabled {
        tracing::info!("Etherscan verifier is enabled");
        let etherscan_verifier = EtherscanVerifier::new(
            verifier_config.etherscan_api_url.unwrap(),
            etherscan_api_key.unwrap(),
            pool.clone(),
            stop_receiver.clone(),
        );
        let etherscan_verifier_metrics = EtherscanVerifierMetrics::new(pool, stop_receiver);
        tasks.push(tokio::spawn(etherscan_verifier.run()));
        tasks.push(tokio::spawn(etherscan_verifier_metrics.run()));
    } else {
        tracing::info!("Etherscan verifier is disabled");
    }

    let mut tasks = ManagedTasks::new(tasks);
    tokio::select! {
        () = tasks.wait_single() => {},
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Stop request received, shutting down");
        },
    };
    stop_sender.send_replace(true);

    // Sleep for some time to let verifier gracefully stop.
    tasks.complete(Duration::from_secs(5)).await;
    Ok(())
}
