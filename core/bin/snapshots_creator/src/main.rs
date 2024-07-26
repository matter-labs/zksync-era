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
use zksync_config::configs::PrometheusConfig;
use zksync_core_leftovers::temp_config_store::{load_database_secrets, load_general_config};
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStoreFactory;
use zksync_vlog::prometheus::PrometheusExporterConfig;

use crate::creator::SnapshotCreator;

mod creator;
mod metrics;
#[cfg(test)]
mod tests;

async fn maybe_enable_prometheus_metrics(
    prometheus_config: Option<PrometheusConfig>,
    stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<Option<JoinHandle<anyhow::Result<()>>>> {
    match prometheus_config.map(|c| (c.gateway_endpoint(), c.push_interval())) {
        Some((Some(gateway_endpoint), push_interval)) => {
            tracing::info!("Starting prometheus exporter with gateway {gateway_endpoint:?} and push_interval {push_interval:?}");
            let exporter_config = PrometheusExporterConfig::push(gateway_endpoint, push_interval);

            let prometheus_exporter_task = tokio::spawn(exporter_config.run(stop_receiver));
            Ok(Some(prometheus_exporter_task))
        }
        _ => {
            tracing::info!("Starting without prometheus exporter");
            Ok(None)
        }
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
    let general_config = load_general_config(opt.config_path).context("general config")?;
    let database_secrets = load_database_secrets(opt.secrets_path).context("database secrets")?;

    let observability_config = general_config
        .observability
        .context("observability config")?;
    let _observability_guard = observability_config.install()?;
    let prometheus_exporter_task =
        maybe_enable_prometheus_metrics(general_config.prometheus_config, stop_receiver).await?;
    tracing::info!("Starting snapshots creator");

    let creator_config = general_config
        .snapshot_creator
        .context("snapshot creator config")?;

    let object_store_config = creator_config
        .clone()
        .object_store
        .context("snapshot creator object storage config")?;

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
