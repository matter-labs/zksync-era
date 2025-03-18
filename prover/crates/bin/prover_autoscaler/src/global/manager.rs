use std::collections::HashMap;

use super::{
    queuer,
    scaler::{Scaler, ScalerTrait},
    watcher,
};
use crate::{
    agent::ScaleRequest,
    cluster_types::{ClusterName, Clusters, NamespaceName},
    config::{ProverAutoscalerScalerConfig, QueueReportFields, ScalerTargetType},
    key::{GpuKey, NoKey},
    metrics::AUTOSCALER_METRICS,
    task_wiring::Task,
};

pub struct Manager {
    /// Namespace to Protocol Version configuration.
    namespaces: HashMap<NamespaceName, String>,
    watcher: watcher::Watcher,
    queuer: queuer::Queuer,

    jobs: Vec<QueueReportFields>,
    scalers: Vec<Box<dyn ScalerTrait + Sync + Send>>,
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

        let mut scalers: Vec<Box<dyn ScalerTrait + Sync + Send>> = Vec::default();
        let mut jobs = Vec::default();
        for c in &config.scaler_targets {
            jobs.push(c.queue_report_field);
            match c.scaler_target_type {
                ScalerTargetType::Gpu => scalers.push(Box::new(Scaler::<GpuKey>::new(
                    c.queue_report_field,
                    c.deployment.clone(),
                    c.min_replicas,
                    c.max_replicas
                        .iter()
                        .map(|(k, v)| (k.clone(), v.into_map_gpukey()))
                        .collect(),
                    c.speed.into_map_gpukey(),
                    config.cluster_priorities.clone(),
                    config.apply_min_to_namespace.clone(),
                    chrono::Duration::seconds(config.long_pending_duration.as_secs() as i64),
                ))),
                ScalerTargetType::Simple => scalers.push(Box::new(Scaler::<NoKey>::new(
                    c.queue_report_field,
                    c.deployment.clone(),
                    c.min_replicas,
                    c.max_replicas
                        .iter()
                        .map(|(k, v)| (k.clone(), v.into_map_nokey()))
                        .collect(),
                    c.speed.into_map_nokey(),
                    config.cluster_priorities.clone(),
                    config.apply_min_to_namespace.clone(),
                    chrono::Duration::seconds(config.long_pending_duration.as_secs() as i64),
                ))),
            };
        }
        Self {
            namespaces: config.protocol_versions.clone(),
            watcher,
            queuer,
            jobs,
            scalers,
        }
    }
}

/// is_namespace_running returns true if there are some pods running in it.
fn is_namespace_running(namespace: &NamespaceName, clusters: &Clusters) -> bool {
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

        let mut scale_requests: HashMap<ClusterName, ScaleRequest> = HashMap::new();
        {
            let guard = self.watcher.data.lock().await; // Keeping the lock during all calls of run() for
                                                        // consitency.
            if let Err(err) = watcher::check_is_ready(&guard.is_ready) {
                AUTOSCALER_METRICS.clusters_not_ready.inc();
                tracing::warn!("Skipping Manager run: {}", err);
                return Ok(());
            }

            for (ns, ppv) in &self.namespaces {
                for scaler in &self.scalers {
                    let q = queue
                        .get(&(ppv.to_string(), scaler.queue_report_field()))
                        .cloned()
                        .unwrap_or(0);
                    AUTOSCALER_METRICS.queue[&(ns.clone(), scaler.deployment())].set(q);
                    tracing::debug!(
                        "Running eval for namespace {ns}, PPV {ppv}, scaler {} found queue {q}",
                        scaler.deployment()
                    );
                    if q > 0 || is_namespace_running(ns, &guard.clusters) {
                        scaler.run(ns, q, &guard.clusters, &mut scale_requests);
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
