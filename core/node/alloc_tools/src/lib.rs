//! Jemalloc tooling used by ZKsync node.

use std::time::Duration;

use anyhow::Context;
use serde::Serialize;
use tikv_jemalloc_ctl::stats_print;
use tokio::sync::watch;
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};

pub use self::node::JemallocMonitorLayer;
use crate::{
    json::{GeneralStats, JemallocStats},
    metrics::METRICS,
};

mod json;
mod metrics;
mod node;

#[derive(Debug, Serialize)]
struct JemallocHealthDetails {
    version: &'static str,
    config: &'static str,
    stats: GeneralStats,
}

/// Monitors general Jemalloc statistics and reports them as a health check, logs and metrics.
#[derive(Debug)]
pub(crate) struct JemallocMonitor {
    version: &'static str,
    config: &'static str,
    health: HealthUpdater,
    update_interval: Duration,
}

impl JemallocMonitor {
    pub(crate) fn new() -> anyhow::Result<Self> {
        let version =
            tikv_jemalloc_ctl::version::read().context("failed reading Jemalloc version")?;
        let config = tikv_jemalloc_ctl::config::malloc_conf::read()
            .context("failed reading Jemalloc config")?;

        Ok(Self {
            version,
            config,
            health: ReactiveHealthCheck::new("jemalloc").1,
            update_interval: Duration::from_secs(60),
        })
    }

    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.health.subscribe()
    }

    fn update(&self) -> anyhow::Result<()> {
        let mut options = stats_print::Options::default();
        options.json_format = true;
        options.skip_constants = true;
        options.skip_per_arena = true;
        options.skip_bin_size_classes = true;
        options.skip_large_size_classes = true;
        options.skip_mutex_statistics = true;

        let mut buffer = vec![];
        stats_print::stats_print(&mut buffer, options)
            .context("failed collecting Jemalloc stats")?;
        let JemallocStats::Jemalloc { stats, arena_stats } =
            serde_json::from_slice(&buffer).context("failed deserializing Jemalloc stats")?;
        METRICS.observe_general_stats(&stats);
        METRICS.observe_arena_stats(&arena_stats);

        let health = Health::from(HealthStatus::Ready).with_details(JemallocHealthDetails {
            version: self.version,
            config: self.config,
            stats,
        });
        self.health.update(health);
        Ok(())
    }

    pub(crate) async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        tracing::info!(
            version = self.version,
            config = self.config,
            "Initializing Jemalloc monitor"
        );

        while !*stop_receiver.borrow() {
            if let Err(err) = self.update() {
                tracing::warn!("Error updating Jemalloc stats: {err:#}");
            }

            if tokio::time::timeout(self.update_interval, stop_receiver.changed())
                .await
                .is_ok()
            {
                break;
            }
        }
        tracing::info!("Received stop signal, Jemalloc monitor is shutting down");
        Ok(())
    }
}
