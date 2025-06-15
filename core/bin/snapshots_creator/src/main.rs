//! Snapshot creator utility. Intended to run on a schedule, with each run creating a new snapshot.
//!
//! # Assumptions
//!
//! The snapshot creator is fault-tolerant; if it stops in the middle of creating a snapshot,
//! this snapshot will be continued from roughly the same point after the restart. If this is
//! undesired, remove the `snapshots` table record corresponding to the pending snapshot.
//!
//! It is assumed that the snapshot creator is run as a singleton process (no more than 1 instance
//! at a time).

use anyhow::Context as _;
use structopt::StructOpt;
use tokio::{sync::watch, task::JoinHandle};
use zksync_config::{
    configs::{DatabaseSecrets, PrometheusConfig},
    full_config_schema,
    sources::ConfigFilePaths,
    SnapshotsCreatorConfig,
};
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStoreFactory;

use crate::creator::SnapshotCreator;

mod creator;
mod metrics;
#[cfg(test)]
mod tests;

fn maybe_enable_prometheus_metrics(
    config: &PrometheusConfig,
    stop_receiver: watch::Receiver<bool>,
) -> Option<JoinHandle<anyhow::Result<()>>> {
    if let Some(exporter_config) = config.to_exporter_config() {
        tracing::info!("Starting Prometheus exporter with config {config:?}");
        let prometheus_exporter_task = tokio::spawn(exporter_config.run(stop_receiver));
        Some(prometheus_exporter_task)
    } else {
        tracing::info!("Starting without prometheus exporter");
        None
    }
}

/// Minimum number of storage log chunks to produce.
const MIN_CHUNK_COUNT: u64 = 10;

#[derive(StructOpt)]
#[structopt(name = "ZKsync snapshot creator", author = "Matter Labs")]
struct Opt {
    /// Path to the configuration file.
    #[structopt(long)]
    config_path: Option<std::path::PathBuf>,

    /// Path to the secrets file.
    #[structopt(long)]
    secrets_path: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (stop_sender, stop_receiver) = watch::channel(false);

    let opt = Opt::from_args();
    let config_file_paths = ConfigFilePaths {
        general: opt.config_path,
        secrets: opt.secrets_path,
        ..ConfigFilePaths::default()
    };
    let config_sources =
        tokio::task::spawn_blocking(|| config_file_paths.into_config_sources("ZKSYNC_")).await??;

    let _observability_guard = config_sources.observability()?.install()?;

    let schema = full_config_schema(false);
    let mut repo = config_sources.build_repository(&schema);
    let database_secrets: DatabaseSecrets = repo.parse()?;
    let creator_config: SnapshotsCreatorConfig = repo.parse()?;
    let prometheus_config: PrometheusConfig = repo.parse()?;

    let prometheus_exporter_task =
        maybe_enable_prometheus_metrics(&prometheus_config, stop_receiver);
    tracing::info!("Starting snapshots creator");

    let object_store_config = creator_config.object_store.clone();
    let blob_store = ObjectStoreFactory::new(object_store_config)
        .create_store()
        .await?;

    let replica_pool = ConnectionPool::<Core>::builder(
        database_secrets.replica_url()?,
        creator_config.concurrent_queries_count,
    )
    .build()
    .await?;

    let master_pool = ConnectionPool::<Core>::singleton(database_secrets.master_url()?)
        .build()
        .await?;

    let creator = SnapshotCreator {
        blob_store,
        master_pool,
        replica_pool,
        #[cfg(test)]
        event_listener: Box::new(()),
    };
    creator.run(creator_config, MIN_CHUNK_COUNT).await?;

    tracing::info!("Finished running snapshot creator!");
    stop_sender.send(true).ok();
    if let Some(prometheus_exporter_task) = prometheus_exporter_task {
        prometheus_exporter_task
            .await?
            .context("Prometheus did not finish gracefully")?;
    }
    Ok(())
}
