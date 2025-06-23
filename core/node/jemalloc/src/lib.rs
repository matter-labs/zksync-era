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
    compile_time_config: &'static str,
    runtime_config: serde_json::Value,
    stats: GeneralStats,
}

/// Monitors general Jemalloc statistics and reports them as a health check, logs and metrics.
#[derive(Debug)]
pub(crate) struct JemallocMonitor {
    version: &'static str,
    compile_time_config: &'static str,
    runtime_config: Option<serde_json::Value>,
    health: HealthUpdater,
    update_interval: Duration,
}

impl JemallocMonitor {
    const DEFAULT_UPDATE_INTERVAL: Duration = Duration::from_secs(60);

    pub(crate) fn new() -> anyhow::Result<Self> {
        let version =
            tikv_jemalloc_ctl::version::read().context("failed reading Jemalloc version")?;
        let version = version.strip_suffix('\0').unwrap_or(version);
        let config = tikv_jemalloc_ctl::config::malloc_conf::read()
            .context("failed reading compile-time Jemalloc config")?;
        let config = config.strip_suffix('\0').unwrap_or(config);

        Ok(Self {
            version,
            compile_time_config: config,
            runtime_config: None, // to be overwritten on the first `update()`
            health: ReactiveHealthCheck::new("jemalloc").1,
            update_interval: Self::DEFAULT_UPDATE_INTERVAL,
        })
    }

    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.health.subscribe()
    }

    fn update(&mut self) -> anyhow::Result<()> {
        let mut options = stats_print::Options::default();
        options.json_format = true;
        options.skip_constants = self.runtime_config.is_some();
        options.skip_per_arena = true;
        options.skip_bin_size_classes = true;
        options.skip_large_size_classes = true;
        options.skip_mutex_statistics = true;

        let mut buffer = vec![];
        stats_print::stats_print(&mut buffer, options)
            .context("failed collecting Jemalloc stats")?;
        let JemallocStats::Jemalloc {
            opt,
            stats,
            arena_stats,
        } = serde_json::from_slice(&buffer).context("failed deserializing Jemalloc stats")?;
        METRICS.observe_general_stats(&stats);
        METRICS.observe_arena_stats(&arena_stats);

        let runtime_config = &*self.runtime_config.get_or_insert_with(|| {
            let opt = opt.unwrap_or_else(|| serde_json::json!({}));
            tracing::info!(%opt, "Read Jemalloc runtime config");
            opt
        });

        let health = Health::from(HealthStatus::Ready).with_details(JemallocHealthDetails {
            version: self.version,
            compile_time_config: self.compile_time_config,
            runtime_config: runtime_config.clone(),
            stats,
        });
        self.health.update(health);
        Ok(())
    }

    pub(crate) async fn run(
        mut self,
        mut stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        tracing::info!(
            version = self.version,
            config = self.compile_time_config,
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
