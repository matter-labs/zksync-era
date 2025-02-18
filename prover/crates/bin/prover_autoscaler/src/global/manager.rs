use std::collections::HashMap;

use super::{queuer, scaler::Scaler, watcher};
use crate::{
    agent::ScaleRequest,
    cluster_types::Clusters,
    config::{ProverAutoscalerScalerConfig, QueueReportFields},
    key::{GpuKey, NoKey},
    metrics::AUTOSCALER_METRICS,
    task_wiring::Task,
};

pub struct Manager {
    /// namespace to Protocol Version configuration.
    namespaces: HashMap<String, String>,
    watcher: watcher::Watcher,
    queuer: queuer::Queuer,

    jobs: Vec<QueueReportFields>,
    gpu_scalers: Vec<Scaler<GpuKey>>,
    scalers: Vec<Scaler<NoKey>>,
}

impl Manager {
    pub fn new(
        watcher: watcher::Watcher,
        queuer: queuer::Queuer,
        config: ProverAutoscalerScalerConfig,
    ) -> Self {
        config
            .protocol_versions
            .iter()
            .for_each(|(namespace, version)| {
                AUTOSCALER_METRICS.prover_protocol_version[&(namespace.clone(), version.clone())]
                    .set(1);
            });

        let mut gpu_scalers = Vec::default();
        let mut scalers = Vec::default();
        let mut jobs = vec![];
        for c in &config.gpu_scaler_targets {
            jobs.push(c.queue_report_field);
            gpu_scalers.push(Scaler::new(
                c,
                config.cluster_priorities.clone(),
                config.apply_min_to_namespace.clone(),
                chrono::Duration::seconds(config.long_pending_duration.as_secs() as i64),
            ))
        }
        for c in &config.scaler_targets {
            jobs.push(c.queue_report_field);
            scalers.push(Scaler::new(
                c,
                config.cluster_priorities.clone(),
                config.apply_min_to_namespace.clone(),
                chrono::Duration::seconds(config.long_pending_duration.as_secs() as i64),
            ))
        }
        Self {
            namespaces: config.protocol_versions.clone(),
            watcher,
            queuer,
            jobs,
            gpu_scalers,
            scalers,
        }
    }
}
/// is_namespace_running returns true if there are some pods running in it.
fn is_namespace_running(namespace: &str, clusters: &Clusters) -> bool {
    clusters
        .clusters
        .values()
        .flat_map(|v| v.namespaces.iter())
        .filter_map(|(k, v)| if k == namespace { Some(v) } else { None })
        .flat_map(|v| v.deployments.values())
        .map(
            |d| d.running + d.desired, // If there is something running or expected to run, we
                                       // should re-evaluate the namespace.
        )
        .sum::<i32>()
        > 0
}

#[async_trait::async_trait]
impl Task for Manager {
    async fn invoke(&self) -> anyhow::Result<()> {
        let queue = self.queuer.get_queue(&self.jobs).await.unwrap();

        let mut scale_requests: HashMap<String, ScaleRequest> = HashMap::new();
        {
            let guard = self.watcher.data.lock().await; // Keeping the lock during all calls of run() for
                                                        // consitency.
            if let Err(err) = watcher::check_is_ready(&guard.is_ready) {
                AUTOSCALER_METRICS.clusters_not_ready.inc();
                tracing::warn!("Skipping Manager run: {}", err);
                return Ok(());
            }

            for (ns, ppv) in &self.namespaces {
                // Prover
                for scaler in &self.gpu_scalers {
                    let q = queue
                        .get(&(ppv.to_string(), scaler.queue_report_field))
                        .cloned()
                        .unwrap_or(0);
                    AUTOSCALER_METRICS.queue[&(ns.clone(), "prover".into())].set(q);
                    tracing::debug!(
                        "Running eval for namespace {ns}, PPV {ppv}, gpu scaler {} found queue {q}",
                        scaler.deployment
                    );
                    if q > 0 || is_namespace_running(ns, &guard.clusters) {
                        let replicas = scaler.run(ns, q, &guard.clusters);
                        for (k, num) in &replicas {
                            AUTOSCALER_METRICS.provers[&(k.cluster.clone(), ns.clone(), k.key.0)]
                                .set(*num as u64);
                        }
                        scaler.diff(ns, replicas, &guard.clusters, &mut scale_requests);
                    }
                }

                // Simple Scalers.
                for scaler in &self.scalers {
                    let q = queue
                        .get(&(ppv.to_string(), scaler.queue_report_field))
                        .cloned()
                        .unwrap_or(0);
                    AUTOSCALER_METRICS.queue[&(ns.clone(), scaler.deployment.clone())].set(q);
                    tracing::debug!(
                        "Running eval for namespace {ns}, PPV {ppv}, scaler {} found queue {q}",
                        scaler.deployment
                    );
                    if q > 0 || is_namespace_running(ns, &guard.clusters) {
                        let replicas = scaler.run(ns, q, &guard.clusters);
                        for (k, num) in &replicas {
                            AUTOSCALER_METRICS.jobs
                                [&(scaler.deployment.clone(), k.cluster.clone(), ns.clone())]
                                .set(*num as u64);
                        }
                        scaler.diff(ns, replicas, &guard.clusters, &mut scale_requests);
                    }
                }
            }
        } // Unlock self.watcher.data.

        if let Err(err) = self.watcher.send_scale(scale_requests).await {
            tracing::error!("Failed scale request: {}", err);
        }

        Ok(())
    }
}
